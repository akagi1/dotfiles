# Claude Code skills

Each folder here is a Claude Code skill (a `SKILL.md` with frontmatter):

- `translate/`: `/translate` translates text into Polish (Polish input into
  English), then breaks it down word by word.

## Setup

Claude Code loads personal skills from `~/.claude/skills/<name>/`. Symlink each
skill folder there instead of copying it, so edits in this repository apply
right away. Link individual skills, not the whole `skills` folder, so other
skills already in `~/.claude/skills` are kept. If a folder with the same name
already exists there, back it up and remove it first.

Windows (PowerShell, from the checkout root):

```powershell
New-Item -ItemType Directory -Force "$HOME\.claude\skills"
New-Item -ItemType SymbolicLink -Path "$HOME\.claude\skills\translate" -Target "$PWD\skills\translate"
```

Symbolic links need Developer Mode (**Settings → System → For developers**) or
an elevated shell. Without either, use `-ItemType Junction` instead; it works
the same for folders and needs no extra rights.

macOS/Linux (from the checkout root):

```sh
mkdir -p ~/.claude/skills
ln -s "$PWD/skills/translate" ~/.claude/skills/translate
```

Restart Claude Code after adding a new skill so `/translate` shows up. When
adding a skill here, add its folder to the list above and link it the same way.
