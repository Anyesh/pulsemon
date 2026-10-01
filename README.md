# pulsemon

Cross-platform system monitor TUI.

Monitors CPU, memory, disk, GPU, processes and ports, sorts every table column, and opens any process in an inspector that shows who started it, who it runs as and how it runs. Works with the keyboard or the mouse.

## Install

**Download a binary** from [Releases](https://github.com/Anyesh/pulsemon/releases): pick your platform and run it.

**Or build from source:**

```
cargo install --path .
```

## Usage

```
pulsemon              # launch with defaults
pulsemon --rate 500   # 500ms refresh rate
pulsemon --no-gpu     # skip GPU detection
pulsemon --no-ports   # skip port scanning
pulsemon --no-mouse   # start with mouse capture off
pulsemon --debug-timing   # show how long each collection pass takes
```

## Keybindings

| Key | Action |
|-----|--------|
| `1-7` | Jump to view (Dashboard, CPU, Memory, Disk, GPU, Processes, Ports) |
| `Tab` / `Shift+Tab` | Cycle views |
| `j/k` or `↑/↓`, `PgUp/PgDn`, `Home/End` | Move the selection |
| `s` / `S` | Next sort column / flip direction |
| `/` | Filter the current table (each table keeps its own filter) |
| `Enter` | Inspect the selected process, or the owner of the selected port |
| `K` or `Del` | Signal menu for the selection |
| `m` | Mouse capture on or off |
| `:` | Command palette |
| `+` / `-` | Faster / slower refresh |
| `?` | Help |
| `Esc` | Clear the filter, go back to the dashboard, then quit |
| `q` | Quit |

## Mouse

Click a tab, a dashboard panel or a table row; click a column header to sort by it and click it again to flip the direction. Double-click a row to inspect it, right-click a row for the signal menu, and use the wheel to scroll whichever table is under the pointer without moving the selection.

Mouse capture stops the terminal from selecting text. Hold Shift while dragging to select anyway, or press `m` to turn capture off and on again.

## Process inspector

`Enter`, a double-click or `:inspect <pid>` opens the inspector:

- **Identity**: executable, arguments, working directory, start time and uptime.
- **Ownership**: user, effective user (flagged when setuid), group, session, and on Linux the login user from `loginuid`, so `sudo` shows who actually ran it.
- **Lineage**: the parent chain and children, both clickable. A parent that started after its child is reported as pid reuse, and a process whose parent is init or a subreaper is flagged as possibly orphaned.
- **Ports** the process owns, and its **environment**, hidden until you press `e` because it often holds secrets.
- **Linux**: systemd unit and slice or container (docker, containerd, podman, Kubernetes pods) from cgroup v1 or v2, namespaces it does not share with pulsemon, effective capabilities, threads, open fds against the limit, nice, OOM score, PSS, swap and tty.
- **Windows**: services hosted by the process (useful for `svchost`), token integrity level and elevation, and whether it runs in the services session, at the console or over remote desktop.

Fields the current user may not read show as `denied` instead of failing the view. `Backspace` goes back through the processes you followed and `Esc` closes the inspector.

## Signals

`K`, `Del`, a right-click, `:kill` and `:kill-port` all open a menu with TERM, HUP, INT, STOP, CONT and KILL (only Terminate on Windows). The menu is bound to the process's pid and start time, and pulsemon checks the start time again right before sending, so a pid that was reused in the meantime is never signalled. On Linux the check and the send go through a pidfd, which closes the remaining race.

## Commands

Type `:` to open the command palette:

```
:inspect 1234       Open the inspector for a PID
:kill 1234          Signal menu for a PID
:kill-port 3000     Signal menu for the process on a port
:sort mem           Sort by a column: pid, user, name, cpu, mem, disk, status, command,
                    or for ports proto, local, port, remote, state, pid, process
:rate 500           Set refresh rate in ms
:filter chrome      Filter the current table
:q                  Quit
```

## GPU Support

| Vendor | Method | Platforms |
|--------|--------|-----------|
| NVIDIA | NVML (native) | Windows, Linux, macOS |
| AMD | rocm-smi (CLI) | Linux |
| Intel | intel_gpu_top (CLI) | Linux |
| Apple | ioreg / powermetrics | macOS |

GPU detection is best-effort — if your GPU isn't supported, pulsemon shows "No GPU detected" and everything else works fine.

## Extending

The codebase uses traits for pluggable backends:

- **GPU** — implement `GpuBackend` in `src/collectors/gpu/` and register in `detect_gpus()`
- **Ports** — implement `PortScanner` in `src/collectors/ports/` with platform-specific parsing
- **Views** — add a new view file in `src/ui/`, add a variant to `View` enum in `src/app/mod.rs`

All colors live in `src/theme.rs` if you want to change the palette.

## Releasing

Push a tag to trigger a release build:

```
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions builds binaries for Linux, macOS (amd64 + arm64), and Windows, then creates a release.

## License

MIT
