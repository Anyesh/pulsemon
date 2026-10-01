use super::action::Action;
use crate::views::port_view::PortColumn;
use crate::views::process_view::ProcessColumn;

/// Parses a command palette line. `Ok(None)` means the line was blank; `Err` carries
/// the message to show in the status bar. Column names shared by both tables (such as
/// `pid`) go to the ports table only when `ports_first` is set.
pub fn parse(line: &str, ports_first: bool) -> Result<Option<Action>, String> {
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
            sort_action(&col.to_lowercase(), ports_first)
                .ok_or_else(|| format!("Unknown sort column: {col}"))?
        }
        _ => return Err(format!("Unknown command: {name}")),
    };
    Ok(Some(action))
}

fn sort_action(col: &str, ports_first: bool) -> Option<Action> {
    let process = || ProcessColumn::from_name(col).map(Action::SortProcesses);
    let port = || PortColumn::from_name(col).map(Action::SortPorts);
    if ports_first {
        port().or_else(process)
    } else {
        process().or_else(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_line_is_no_action() {
        assert_eq!(parse("   ", false), Ok(None));
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse("quit", false), Ok(Some(Action::Quit)));
        assert_eq!(parse("kill 42", false), Ok(Some(Action::Kill(42))));
        assert_eq!(
            parse("kill-port 8080", false),
            Ok(Some(Action::KillPort(8080)))
        );
        assert_eq!(parse("rate 500", false), Ok(Some(Action::SetRate(500))));
        assert_eq!(
            parse("filter foo bar", false),
            Ok(Some(Action::SetFilter("foo bar".into())))
        );
        assert_eq!(
            parse("sort MEM", false),
            Ok(Some(Action::SortProcesses(ProcessColumn::Memory)))
        );
    }

    #[test]
    fn shared_column_names_follow_the_current_table() {
        assert_eq!(
            parse("sort pid", false),
            Ok(Some(Action::SortProcesses(ProcessColumn::Pid)))
        );
        assert_eq!(
            parse("sort pid", true),
            Ok(Some(Action::SortPorts(PortColumn::Pid)))
        );
        assert_eq!(
            parse("sort command", true),
            Ok(Some(Action::SortProcesses(ProcessColumn::Command)))
        );
        assert_eq!(
            parse("sort remote", false),
            Ok(Some(Action::SortPorts(PortColumn::Remote)))
        );
    }

    #[test]
    fn reports_usage_and_invalid_input() {
        assert_eq!(parse("kill", false), Err("Usage: kill <pid>".into()));
        assert_eq!(parse("kill abc", false), Err("Invalid PID".into()));
        assert_eq!(parse("rate 50", false), Err("Minimum rate is 100ms".into()));
        assert_eq!(
            parse("sort bogus", false),
            Err("Unknown sort column: bogus".into())
        );
        assert_eq!(parse("frob", false), Err("Unknown command: frob".into()));
    }
}
