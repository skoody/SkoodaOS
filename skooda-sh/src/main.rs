use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::fs::OpenOptions;
use std::process::{Command, Stdio};
use os_pipe::pipe;
use tracing::{error, info, warn};
use skooda_utils::logging::init_logging;
use skooda_utils::error::{Result, SkoodaError};
use std::path::{Path, PathBuf};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    init_logging();
    info!("--- SkoodaOS Shell v0.2.2 (Path & Builtin Refactor) ---");
    
    let mut rl = DefaultEditor::new()?;
    
    loop {
        let readline = rl.readline("skooda> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() { continue; }
                if line == "exit" { break; }
                let _ = rl.add_history_entry(line);

                if let Err(e) = execute_pipeline(line) {
                    eprintln!("skooda-sh: {}", e);
                    error!("Command failed: {}", e);
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                error!("Readline error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}

fn execute_pipeline(line: &str) -> Result<()> {
    let stages: Vec<&str> = line.split('|').collect();
    let mut commands = Vec::new();

    // 1. Parsing Phase
    for stage in stages {
        let words = shlex::split(stage).ok_or_else(|| SkoodaError::System("Invalid quoting".into()))?;
        let (cmd_args, redir) = parse_redirection(words)?;
        if cmd_args.is_empty() { continue; }
        commands.push((cmd_args, redir));
    }

    if commands.is_empty() { return Ok(()); }

    // 2. Builtin Check (Single command only)
    if commands.len() == 1 {
        let (args, _) = &commands[0];
        match args[0].as_str() {
            "cd" => {
                let new_dir = args.get(1).map(|s| s.as_str()).unwrap_or("/");
                std::env::set_current_dir(new_dir).map_err(|e| SkoodaError::Io {
                    path: new_dir.into(),
                    source: e,
                })?;
                return Ok(());
            }
            "echo" => {
                println!("{}", args[1..].join(" "));
                return Ok(());
            }
            _ => {}
        }
    }

    // 3. Execution Phase
    let mut prev_pipe: Option<os_pipe::PipeReader> = None;
    let mut children = Vec::new();

    for (i, (cmd_args, redir)) in commands.iter().enumerate() {
        let is_last = i == commands.len() - 1;
        
        let exe_path = find_executable(&cmd_args[0]).ok_or_else(|| {
            SkoodaError::System(format!("Command not found: {}", cmd_args[0]))
        })?;

        let mut command = Command::new(exe_path);
        command.args(&cmd_args[1..]);

        // Stdin
        if let Some(path) = &redir.stdin {
            let file = OpenOptions::new().read(true).open(path).map_err(|e| SkoodaError::Io {
                path: path.into(),
                source: e,
            })?;
            command.stdin(Stdio::from(file));
        } else if let Some(reader) = prev_pipe {
            command.stdin(Stdio::from(reader));
        }

        // Stdout
        let (next_reader, stdout_stdio) = if !is_last {
            let (reader, writer) = pipe().map_err(|e| SkoodaError::System(format!("Pipe failed: {}", e)))?;
            (Some(reader), Stdio::from(writer))
        } else if let Some(path) = &redir.stdout {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(redir.append)
                .truncate(!redir.append)
                .open(path)
                .map_err(|e| SkoodaError::Io {
                    path: path.into(),
                    source: e,
                })?;
            (None, Stdio::from(file))
        } else {
            (None, Stdio::inherit())
        };
        command.stdout(stdout_stdio);

        // Stderr
        if let Some(path) = &redir.stderr {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(redir.append)
                .truncate(!redir.append)
                .open(path)
                .map_err(|e| SkoodaError::Io {
                    path: path.into(),
                    source: e,
                })?;
            command.stderr(Stdio::from(file));
        }

        match command.spawn() {
            Ok(child) => {
                children.push(child);
                prev_pipe = next_reader;
            }
            Err(e) => {
                return Err(SkoodaError::System(format!("{}: {}", cmd_args[0], e)));
            }
        }
    }

    for mut child in children {
        if let Err(e) = child.wait() {
            warn!("Child process error: {}", e);
        }
    }

    Ok(())
}

fn find_executable(cmd: &str) -> Option<PathBuf> {
    if cmd.starts_with('/') || cmd.starts_with("./") {
        let p = PathBuf::from(cmd);
        if p.exists() { return Some(p); }
        return None;
    }

    let path_env = std::env::var("PATH").unwrap_or_else(|_| "/bin:/usr/bin:/sbin".to_string());
    for dir in path_env.split(':') {
        let p = Path::new(dir).join(cmd);
        if p.exists() { return Some(p); }
    }
    None
}

struct Redirection {
    stdin: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    append: bool,
}

fn parse_redirection(words: Vec<String>) -> Result<(Vec<String>, Redirection)> {
    let mut cmd_args = Vec::new();
    let mut redir = Redirection {
        stdin: None,
        stdout: None,
        stderr: None,
        append: false,
    };
    
    let mut i = 0;
    while i < words.len() {
        match words[i].as_str() {
            "<" => {
                if i + 1 < words.len() {
                    redir.stdin = Some(words[i + 1].clone());
                    i += 2;
                } else {
                    return Err(SkoodaError::System("Missing file for <".into()));
                }
            }
            ">" => {
                if i + 1 < words.len() {
                    redir.stdout = Some(words[i + 1].clone());
                    redir.append = false;
                    i += 2;
                } else {
                    return Err(SkoodaError::System("Missing file for >".into()));
                }
            }
            ">>" => {
                if i + 1 < words.len() {
                    redir.stdout = Some(words[i + 1].clone());
                    redir.append = true;
                    i += 2;
                } else {
                    return Err(SkoodaError::System("Missing file for >>".into()));
                }
            }
            "2>" => {
                if i + 1 < words.len() {
                    redir.stderr = Some(words[i + 1].clone());
                    i += 2;
                } else {
                    return Err(SkoodaError::System("Missing file for 2>".into()));
                }
            }
            _ => {
                cmd_args.push(words[i].clone());
                i += 1;
            }
        }
    }
    
    Ok((cmd_args, redir))
}
