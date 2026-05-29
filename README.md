# lumiknit's CLI EDC

My CLI junk/toy tools!

## Build or Install

```bash
cargo build --release

cargo install --path .
```

## Which programs?

### `llm`

Minimal LLM chat completion CLI.
Manages a chat context file (default: `llm.context`)
and streams responses via curl.
Support OpenAI-compatible APIs.

You may need to set some environments:

- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`
- `OPENAI_DEFAULT_MODEL`
- `OPENAI_DEFAULT_SYSTEM_PROMPT`

### `md`

Markdown to ANSI Terminal renderer.
Takes stdin and print colorized document in terminal.
All code blocks are automatically saved to `md-code-<NNN>.<EXT>`

### `ww`

*Wait, what?*
`ww [ext]` works as vipe: reads stdin into a temp file, open `$VISUAL` or `$EDITOR` with the file,
and if the editor closed with exit code 0, prints the result to stdout.
However, this set the file extension as `ext`

## Combinations

To take input from editor, and run llm and print with color,

```sh
ww md | llm | md
```

Modify llm's answer and rerun

```sh
echo "Question!" | llm | ww md | llm
```
