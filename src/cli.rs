
// src/cli.rs
// 命令行解析

use std::collections::HashMap;
use std::env;


pub(crate) struct Command {
    pub(crate) command: Vec<String>,
    pub(crate) options: HashMap<String, Vec<String>>,
}

impl Command {
    pub(crate) fn as_string(&self) -> String {
        let mut result = String::new();
        result.push_str("Command: ");
        result.push_str(&format!("{:?}", self.command));
        result.push_str("\nOptions:\n");
        for (key, values) in &self.options {
            result.push_str(&format!("  {}: {:?}\n", key, values));
        }
        result
    }
}


pub(crate) fn parse_command() -> Result<Command, String> {

    let args: Vec<String> = env::args().skip(1).collect();

    let mut command = Vec::new();
    let mut options = HashMap::new();

    let mut current_option: Option<String> = None;

    for arg in args {
        if arg.starts_with("--") {
            let name = &arg[2..];

            if name.is_empty() {
                return Err("选项名不能为空: --".to_string());
            }

            current_option = Some(name.to_string());
            options.insert(name.to_string(), Vec::new());
        } else if let Some(option) = &current_option {
            options
                .get_mut(option)
                .unwrap()
                .push(arg);
        } else {
            command.push(arg);
        }
    }

    Ok(Command { command, options })
}
