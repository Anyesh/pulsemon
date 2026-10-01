use super::action::Action;
use crate::types::{PortSortBy, ProcessSortBy};

/// Parses a command palette line. `Ok(None)` means the line was blank; `Err` carries
/// the message to show in the status bar.
pub fn parse(line: &str) -> Result<Option<Action>, String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let Some(&name) = parts.first() else {
        return Ok(None);
    };
    let arg = parts.get(1).copied();

    let action = match name {
        "q" | "quit" => Action::Quit,
        "kill" => {
            let arg = arg.ok_or("Usage: kill <pid>")?;
            Action::Kill(arg.parse().map_err(|_| "Invalid PID")?)
        }
        "kill-port" => {
            let arg = arg.ok_or("Usage: kill-port <port>")?;
            Action::KillPort(arg.parse().map_err(|_| "Invalid port number")?)
        }
        "rate" => {
            let arg = arg.ok_or("Usage: rate <ms>")?;
            let ms: u64 = arg.parse().map_err(|_| "Invalid rate value")?;
            if ms < 100 {
                return Err("Minimum rate is 100ms".into());
            }
            Action::SetRate(ms)
        }
        "filter" => Action::SetFilter(parts[1..].join(" ")),
        "sort" => {
            let col = arg.ok_or("Usage: sort <column>")?;
            sort_action(col).ok_or_else(|| format!("Unknown sort column: {col}"))?
        }
        _ => return Err(format!("Unknown command: {name}")),
    };
    Ok(Some(action))
}

fn sort_action(col: &str) -> Option<Action> {
    Some(match col.to_lowercase().as_str() {
        "cpu" => Action::SortProcesses(ProcessSortBy::Cpu),
        "mem" | "memory" => Action::SortProcesses(ProcessSortBy::Memory),
        "pid" => Action::SortProcesses(ProcessSortBy::Pid),
        "name" => Action::SortProcesses(ProcessSortBy::Name),
        "port" => Action::SortPorts(PortSortBy::Port),
        "protocol" => Action::SortPorts(PortSortBy::Protocol),
        "state" => Action::SortPorts(PortSortBy::State),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_line_is_no_action() {
        assert_eq!(parse("   "), Ok(None));
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse("quit"), Ok(Some(Action::Quit)));
        assert_eq!(parse("kill 42"), Ok(Some(Action::Kill(42))));
        assert_eq!(parse("kill-port 8080"), Ok(Some(Action::KillPort(8080))));
        assert_eq!(parse("rate 500"), Ok(Some(Action::SetRate(500))));
        assert_eq!(
            parse("filter foo bar"),
            Ok(Some(Action::SetFilter("foo bar".into())))
        );
        assert_eq!(
            parse("sort MEM"),
            Ok(Some(Action::SortProcesses(ProcessSortBy::Memory)))
        );
    }

    #[test]
    fn reports_usage_and_invalid_input() {
        assert_eq!(parse("kill"), Err("Usage: kill <pid>".into()));
        assert_eq!(parse("kill abc"), Err("Invalid PID".into()));
        assert_eq!(parse("rate 50"), Err("Minimum rate is 100ms".into()));
        assert_eq!(
            parse("sort bogus"),
            Err("Unknown sort column: bogus".into())
        );
        assert_eq!(parse("frob"), Err("Unknown command: frob".into()));
    }
}
