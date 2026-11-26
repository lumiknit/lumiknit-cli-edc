# lumiknit's CLI EDC

My CLI toy but useful tools, which can be used combined with stdin/out as UNIX philosophy!


## Installation

Install rust & cargo first.

```bash
cargo install --git https://github.com/lumiknit/lumiknit-cli-edc
```

Or, clone the repository and run:

```bash
cargo build --release
cargo install --path .
```

## Tools

For more details, please read help message (Run command with `-h`).

- `llm`: Minimal llm chat CLI for any OpenAI compatible API. Require `curl`
- `md`: Streaming markdown to ANSI Terminal renderer. Better with `bat`.
- `ww`: *Wait, what?* Stdin -> `$EDITOR` -> Stdout. Similar to vipe, but receive file extension.
- `jex`: Filter with 'JSON structured with regex`.
- `jflat`: Convert JSON-like (yaml, toml, csv, tsv) data to flat path, and unflatten back!
- `wtr`: Create and clear git worktrees beside the main worktree.
- `lsel`: Stdin only (e.g. `lsel <FILE`). Range → match/context or replace → sort → uniq → byte cap.
- `html-skim`: Filter stdin HTML into Markdown or cleaned HTML for LLM pipelines.

## Example of Combinations

If you want to write question in cli editor then run LLM with pretty printed markdown,

```sh
# Open editor 
ww md | llm | md

# Then, you can run code block with .md-* file
# python3 .md-001.py
```

Run LLM and edit then llm again.

```sh
echo "Question" | llm | ww md | llm
```

Modify Clipboard

```sh
pbpaste | llm | pbcopy
```

Filter some events from the logs

```sh
cat <<-EOF > events.json
{"message": "/[A-Za-z0-9]+/"}
EOF # Only filter alphanum from plain text, and make as jsonl with message singleton
cat app.log | jex -a events.json | llm -e "Summarize these errors"
```

Destruct and restruct complex data file

```sh
cat value.toml | jflat | grep 'number' | jflat -u yaml
```

Convert data file to env

```sh
cat value.toml | jflat | jflat -u env
```

Convert a headered CSV to JSON

```sh
cat data.csv | jflat -f csv | jflat -u json
```

### html-skim

Filter stdin HTML with html5ever, emitting Markdown by default. No file access,
subprocesses, script execution or network requests.

```sh
curl -sL https://example.org | html-skim --main --limit 16000
html-skim --html < page.html
html-skim --include styles < page.html
html-skim --only scripts --limit 8000 < page.html
```

By default it removes scripts, styles, inline SVG, embeds, hidden content,
event handlers and presentation/JS attributes. Useful head metadata becomes
YAML frontmatter (`--no-metadata` to omit). `--main` prefers a main element,
then an article or common content container, falling back to the body.
`--no-links` keeps Markdown link text without URLs. `--include` and `--only`
accept `scripts`, `styles`, or `assets` (both); external assets stay references.

`--limit BYTES` caps the entire output at a UTF-8 boundary, potentially cutting
Markdown/HTML/YAML syntax. It does not bound input memory: parsing builds a DOM.
HTML output is for document extraction, not browser sanitization; explicitly
retained scripts/styles remain active when opened in a browser.
