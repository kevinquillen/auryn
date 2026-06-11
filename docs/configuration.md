# Configuration

Auryn reads a single TOML configuration file. Every setting has a default, so a
missing file is never an error; a malformed file is a hard error so you can fix
it rather than run with wrong settings.

## Location

The file lives in the platform configuration directory, resolved with
`directories::ProjectDirs`:

| Platform | Path |
| --- | --- |
| Linux | `~/.config/auryn/config.toml` |
| macOS | `~/Library/Application Support/Auryn/config.toml` |
| Windows | `%APPDATA%\Auryn\config.toml` |

Print the exact path with:

```bash
auryn config path
```

## Managing the file

```bash
auryn config path    # print the config file path
auryn config print   # print the effective configuration as TOML
auryn config init    # write a default config file if none exists
auryn config edit    # open the config file in $EDITOR (or $VISUAL)
```

## Settings

```toml
# Number of recent conversational turns kept per session preview.
preview_turns = 6

# Maximum bytes Auryn will read from any single session file. Files larger than
# this are skipped. Default is 16 MiB.
max_file_bytes = 16777216

[providers.claude]
enabled = true
# Optional override for the scan root. When unset, the platform default is used.
# root = "/custom/path/to/.claude/projects"

[providers.codex]
enabled = true
# root = "/custom/path/to/.codex/sessions"

[providers.gemini]
enabled = true
# root = "/custom/path/to/.gemini/tmp"
```

Unknown keys are tolerated, so a configuration written by a newer version of
Auryn does not break an older one.

`preview_turns` and `max_file_bytes` are clamped to hard ceilings when the
configuration is loaded (100 turns and 256 MiB respectively), so an unreasonable
value cannot drive heavy CPU or memory use during scanning.

## Environment overrides

For testing and non-standard installs, the scan root for each provider can be
overridden by an environment variable. The variable takes effect only when the
configuration does not set an explicit `root`.

| Variable | Effect |
| --- | --- |
| `AURYN_CLAUDE_DIR` | Claude scan root |
| `AURYN_CODEX_DIR` | Codex scan root |
| `AURYN_GEMINI_DIR` | Gemini scan root |
| `AURYN_FAKE` | When truthy, registers the synthetic fake provider |
| `AURYN_FAKE_DIR` | Directory of fake-provider session fixtures |

The fake provider is for development. It is registered only when `AURYN_FAKE` is
set, or as a fallback when no real provider is available.
