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
- `jflat`: Convert JSON-like (yaml, toml) data to flat path, and unflatten back!

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
