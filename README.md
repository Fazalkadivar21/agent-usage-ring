# claude-usage-ring

A Quickshell bar widget for the illogical-impulse (ii) config that shows your Claude
plan limits as two concentric rings:

- **outer ring**: 5-hour limit
- **inner ring**: weekly (7-day) limit

The rings show the quota you have **left**, so they shrink as you use it. Each one
changes colour like a battery icon: green above 50% left, yellow from 25 to 50%,
orange from 10 to 25%, red below 10%. A dim track sits behind each ring. Hover
over the rings to see the percent left, the time until reset, and the exact reset
time for both limits.

Two parts:

1. `claude-usage-ring` is a small Rust daemon. It polls Claude's usage endpoint
   and writes `~/.cache/claude-usage/usage.json`.
2. `qml/claudeUsage/` holds the QML widget. It watches that file with `FileView`
   and draws the rings with `Shape`/`PathAngleArc`.

## Build

```sh
cargo build --release          # binary: target/release/claude-usage-ring
cargo install --path .         # or install to ~/.cargo/bin/claude-usage-ring
```

## Run

```sh
claude-usage-ring              # poll every 90 s, write ~/.cache/claude-usage/usage.json
claude-usage-ring --once       # fetch once, print the JSON, exit (exit 1 if not "ok")
claude-usage-ring --mock       # no network: cycle through fake values every 4 s
claude-usage-ring --mock-values 40,8   # fixed fake values: 5-hour 40% left, weekly 8% left
claude-usage-ring --help
```

Options: `-o/--output PATH`, `-c/--credentials PATH`, `-i/--interval SECS`. In live
mode the interval is never shorter than 60 s. Mock mode writes to the same file, so
stop the real daemon while you test, and run it again afterwards to put real data back.

### How it gets the data

On every poll the daemon reads `accessToken` and `expiresAt` from
`~/.claude/.credentials.json` (or `$CLAUDE_CONFIG_DIR/.credentials.json`). It then calls

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <accessToken>
anthropic-beta: oauth-2025-04-20
```

and reads `five_hour.utilization` / `seven_day.utilization` (percent **used**) and
`resets_at`. The token is never logged or written anywhere.

The daemon does **not** refresh the token itself. Claude Code refreshes it and
rewrites the credentials file, and the daemon re-reads that file on every poll. A
second refresher would rotate the refresh token behind Claude Code's back and could
log it out. If the token has expired (you haven't used Claude Code for a while), the
widget says so. Run `claude` once and it recovers on the next poll.

### Errors

- If a poll fails (network down, HTTP error, expired token), the daemon keeps the
  last good values and sets `"status": "stale"` with an `error` message. The rings
  dim and the popup shows the reason.
- Each failure in a row doubles the wait before the next try (90 s, 180 s, ... up to
  15 min). On HTTP 429 the daemon also waits at least as long as `Retry-After` says.
- On startup the daemon loads the last good live values from the JSON file. After a
  reboot with no network you still see your last values (marked stale).
- The widget also marks data stale if the daemon misses its own `next_poll_at` by
  more than 2 minutes, which means the daemon has died. Once a window's `resets_at`
  has passed, the widget shows that ring as full without waiting for the next poll.

### Why a JSON file instead of stdout + `Process`

- The daemon and Quickshell run independently. Quickshell reloads its config often
  (ii reloads on every file save). If Quickshell owned the process, every reload
  would kill and restart it, losing its state and calling the API again.
- The last good value lives on disk. The bar draws it right away on reload or
  reboot, before any network call.
- Every bar instance (one per monitor) reads the same file. A `Process` per bar
  would mean one API poller per monitor.
- You can check it by hand with `cat ~/.cache/claude-usage/usage.json`.
- The daemon writes atomically (temp file + rename), so the widget never reads half
  a file. The widget also re-reads the file every 30 s in case an inotify watch gets lost.

Output format (`schema: 1`):

```json
{
  "schema": 1,
  "source": "api",              // or "mock"
  "status": "ok",               // "ok" | "stale" | "error" (no data at all)
  "error": null,
  "fetched_at": "2026-11-09T05:12:12Z",
  "updated_at": "2026-11-09T05:12:12Z",
  "next_poll_at": "2026-11-09T05:13:42Z",
  "poll_interval_s": 90,
  "five_hour": { "used_pct": 4.0, "remaining_pct": 96.0, "resets_at": "2026-11-09T09:59:59Z" },
  "seven_day": { "used_pct": 9.0, "remaining_pct": 91.0, "resets_at": "2026-11-14T02:59:59Z" }
}
```

## Hook into ii

The widget lives in its own folder, `~/.config/quickshell/ii/modules/ii/bar/claudeUsage/`.
The only change to ii's own files is two lines in `modules/ii/bar/BarContent.qml`:

```qml
import qs.modules.ii.bar.claudeUsage          // with the other imports at the top
```

```qml
                BatteryIndicator {
                    ...
                }

                ClaudeUsage { Layout.alignment: Qt.AlignVCenter }   // right after BatteryIndicator
```

The script does both steps:

```sh
./install-qml.sh            # copy the QML files into ii and reload Quickshell
./install-qml.sh --patch    # same, plus add the two lines to BarContent.qml
                            # (safe to run twice; backs up to BarContent.qml.bak first)
```

If a dotfiles update overwrites `BarContent.qml` or deletes the folder, run
`./install-qml.sh --patch` again.

The script copies the files instead of symlinking them on purpose. With a symlinked
folder, Quickshell doesn't notice edits and keeps running an old copy of the widget.
So after you edit `qml/claudeUsage/*.qml`, run `./install-qml.sh` again.

Widget settings are properties at the top of `ClaudeUsage.qml` (`ringSize`,
`lineWidth`, `dataPath`) and `ClaudeUsageRing.qml` (colours, track opacity). You can
also point the widget at another data file by setting `CLAUDE_USAGE_FILE` in
Quickshell's environment.

## Autostart

**systemd user unit (recommended; restarts the daemon if it crashes):**

```sh
cargo install --path .
mkdir -p ~/.config/systemd/user
cp contrib/claude-usage-ring.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now claude-usage-ring.service
journalctl --user -u claude-usage-ring -f      # logs
```

**Or start it with Hyprland.** This setup uses Lua config. Add this to
`~/.config/hypr/custom/execs.lua`, which dots-hyprland updates don't overwrite:

```lua
hl.on("hyprland.start", function ()
    hl.exec_cmd("$HOME/.cargo/bin/claude-usage-ring")
end)
```

(With a classic `hyprland.conf`, use `exec-once = ~/.cargo/bin/claude-usage-ring`.)

## Caveats

- `api/oauth/usage` is **undocumented** and could change or disappear at any time.
  If the response format changes, the daemon reports `unexpected response shape` and
  the widget shows the last good values as stale.
- It only works with a claude.ai subscription login (Pro/Max OAuth). API-key setups
  have no `claudeAiOauth` entry and no plan limits to show.
- Usage numbers are whole percent and refresh every 90 s, so the rings trail your
  real usage a little.
