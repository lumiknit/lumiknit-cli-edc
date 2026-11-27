use regex::{NoExpand, Regex, RegexBuilder};
use std::collections::VecDeque;
use std::io::{self, BufRead, IsTerminal, Write};

const HELP: &str = include_str!("help.txt");

struct Options {
    ranges: Vec<LineRange>,
    pattern: Option<Regex>,
    replacement: Option<String>,
    fixed: bool,
    invert: bool,
    only: bool,
    context: usize,
    sort: bool,
    uniq: bool,
    limit: Option<usize>,
    max_matches: Option<usize>,
    multiline: bool,
    passthru: bool,
    stats: bool,
    line_numbers: bool,
}

#[derive(Clone, Copy)]
enum Case {
    Sensitive,
    Insensitive,
    Smart,
}

fn parse() -> Result<Option<Options>, String> {
    let mut options = Options {
        ranges: Vec::new(),
        pattern: None,
        replacement: None,
        fixed: false,
        invert: false,
        only: false,
        context: 0,
        sort: false,
        uniq: false,
        limit: None,
        max_matches: None,
        multiline: false,
        passthru: false,
        stats: true,
        line_numbers: io::stdout().is_terminal(),
    };
    let mut args = split_args(std::env::args_os().skip(1))?.into_iter();
    let mut patterns: Vec<String> = Vec::new();
    let mut positional: Vec<String> = Vec::new();
    let mut case = Case::Sensitive;
    let mut has_context = false;
    let mut options_done = false;
    while let Some(arg) = args.next() {
        if options_done || arg == "-" || !arg.starts_with('-') {
            positional.push(arg);
            continue;
        }
        let mut value =
            || args.next().ok_or_else(|| format!("{arg} requires a value"));
        match arg.as_str() {
            "--" => options_done = true,
            "-h" | "--help" => return Ok(None),
            "-e" | "--regexp" => patterns.push(value()?),
            "-l" | "--lines" => options.ranges.extend(line_ranges(&value()?)?),
            "-r" | "--replace" => options.replacement = Some(value()?),
            "-i" | "--ignore-case" => case = Case::Insensitive,
            "-S" | "--smart-case" => case = Case::Smart,
            "-s" | "--case-sensitive" => case = Case::Sensitive,
            "-F" | "--fixed-strings" => options.fixed = true,
            "-v" | "--invert-match" => options.invert = true,
            "-o" | "--only-matching" => options.only = true,
            "-C" | "--context" => {
                has_context = true;
                options.context = count(&value()?)?;
            }
            "--sort" => options.sort = true,
            "--unique" => options.uniq = true,
            "-L" | "--limit" => options.limit = Some(count(&value()?)?),
            "-m" | "--max-count" => {
                options.max_matches = Some(count(&value()?)?)
            }
            "-U" | "--multiline" => options.multiline = true,
            "-k" | "--passthru" | "--passthrough" => options.passthru = true,
            "-n" | "--line-number" => options.line_numbers = true,
            "-N" | "--no-line-number" => options.line_numbers = false,
            "--no-stats" => options.stats = false,
            _ => return Err(format!("unknown option {arg:?} (see --help)")),
        }
    }
    if options.ranges.is_empty() {
        options.ranges.push(LineRange { from: 1, to: None });
    }
    let mut positional = positional.into_iter();
    if patterns.is_empty() {
        patterns.extend(positional.next());
    }
    if let Some(extra) = positional.next() {
        return Err(format!(
            "unexpected argument {extra:?}: lsel reads stdin only (e.g. lsel PATTERN <FILE); use -e for more patterns"
        ));
    }
    if patterns.is_empty()
        && (!matches!(case, Case::Sensitive)
            || options.fixed
            || options.only
            || options.invert
            || has_context
            || options.max_matches.is_some()
            || options.multiline
            || options.passthru
            || options.replacement.is_some())
    {
        return Err(
            "-i, -S, -F, -v, -C, -r, -o, -m, -U and --passthru require a PATTERN".into()
        );
    }
    if options.only && (options.invert || has_context) {
        return Err("-o cannot be combined with -v or -C".into());
    }
    if options.passthru && (options.only || options.invert || has_context) {
        return Err("--passthru cannot be combined with -o, -v or -C".into());
    }
    if options.multiline
        && options.replacement.is_some()
        && !options.passthru
        && (options.invert || has_context)
    {
        return Err("-U -r cannot be combined with -v or -C".into());
    }
    if !patterns.is_empty() {
        let ignore_case = match case {
            Case::Sensitive => false,
            Case::Insensitive => true,
            Case::Smart => !patterns
                .iter()
                .any(|pattern| pattern.chars().any(char::is_uppercase)),
        };
        let pattern = if options.fixed {
            patterns
                .iter()
                .map(|p| regex::escape(p))
                .collect::<Vec<_>>()
        } else {
            patterns
        };
        let pattern = if pattern.len() == 1 {
            pattern.into_iter().next().unwrap()
        } else {
            pattern
                .iter()
                .map(|p| format!("(?:{p})"))
                .collect::<Vec<_>>()
                .join("|")
        };
        options.pattern = Some(
            RegexBuilder::new(&pattern)
                .case_insensitive(ignore_case)
                .multi_line(options.multiline)
                .build()
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(Some(options))
}

struct LineRange {
    from: i64,
    to: Option<i64>,
}

fn line_ranges(value: &str) -> Result<Vec<LineRange>, String> {
    value
        .split(',')
        .map(|range| {
            let (from, to) = match range.split_once("..") {
                Some((from, to)) => (
                    if from.is_empty() {
                        1
                    } else {
                        line_number(from)?
                    },
                    if to.is_empty() {
                        None
                    } else {
                        Some(line_number(to)?)
                    },
                ),
                None => {
                    let number = line_number(range)?;
                    (number, Some(number))
                }
            };
            Ok(LineRange { from, to })
        })
        .collect()
}

fn line_number(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .ok()
        .filter(|&n| n != 0)
        .ok_or_else(|| format!("expected a nonzero line number, got {value:?}"))
}

fn count(value: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("expected a nonnegative count, got {value:?}"))
}

struct Line {
    text: String,
    number: usize,
    context: bool,
}

struct Output<W> {
    writer: W,
    sorted: Option<Vec<Line>>,
    previous: Option<String>,
    uniq: bool,
    remaining: Option<usize>,
    clipped: bool,
    line_numbers: bool,
    group_context: bool,
    last_number: Option<usize>,
    pending_gap: bool,
}

impl<W: Write> Output<W> {
    // Each record carries its original terminator until output.
    fn emit(&mut self, line: Line) -> io::Result<bool> {
        if let Some(sorted) = &mut self.sorted {
            sorted.push(line);
            return Ok(true);
        }
        self.pending_gap |= self.group_context
            && self.last_number.is_some_and(|last| line.number - last > 1);
        self.last_number = Some(line.number);
        let (content, _) = split_line(&line.text);
        if self.uniq && self.previous.as_deref() == Some(content) {
            return Ok(true);
        }
        if self.uniq {
            self.previous = Some(content.to_owned());
        }
        if std::mem::take(&mut self.pending_gap) && !self.write("--\n")? {
            return Ok(false);
        }
        if self.line_numbers {
            let separator = if line.context { '-' } else { ':' };
            if !self.write(&format!("{}{separator}", line.number))? {
                return Ok(false);
            }
        }
        self.write(&line.text)
    }

    fn write(&mut self, record: &str) -> io::Result<bool> {
        let Some(remaining) = &mut self.remaining else {
            self.writer.write_all(record.as_bytes())?;
            return Ok(true);
        };
        let mut end = (*remaining).min(record.len());
        while !record.is_char_boundary(end) {
            end -= 1;
        }
        self.writer.write_all(&record.as_bytes()[..end])?;
        *remaining -= end;
        self.clipped = end < record.len();
        Ok(!self.clipped)
    }

    fn finish(&mut self) -> io::Result<bool> {
        if let Some(mut records) = self.sorted.take() {
            records.sort_by(|a, b| {
                split_line(&a.text).0.cmp(split_line(&b.text).0)
            });
            let total = records.len();
            for (index, mut record) in records.into_iter().enumerate() {
                // An unterminated final input line can move into the middle.
                if index + 1 < total && !record.text.ends_with('\n') {
                    record.text.push('\n');
                }
                if !self.emit(record)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

fn split_line(record: &str) -> (&str, &str) {
    let content = record
        .strip_suffix("\r\n")
        .or_else(|| record.strip_suffix('\n'))
        .unwrap_or(record);
    (content, &record[content.len()..])
}

#[derive(Default)]
struct Stats {
    read: usize,
    scanned: usize,
    matched: usize,
    eof: bool,
}

struct Filter<'a> {
    options: &'a Options,
    before: VecDeque<Line>,
    after: usize,
    matches_left: usize,
    block: Vec<Line>,
    stats: Stats,
}

impl Filter<'_> {
    fn selection_complete(&self) -> bool {
        !self.options.multiline
            && self.matches_left == 0
            && self.after == 0
            && !self.options.passthru
    }

    fn accept(
        &mut self,
        record: String,
        number: usize,
        output: &mut Output<impl Write>,
    ) -> io::Result<bool> {
        if self.options.multiline {
            self.block.push(Line {
                text: record,
                number,
                context: false,
            });
            return Ok(true);
        }
        self.line(record, number, output)
    }

    fn gap(&mut self) {
        self.before.clear();
        self.after = 0;
    }

    fn line(
        &mut self,
        record: String,
        number: usize,
        output: &mut Output<impl Write>,
    ) -> io::Result<bool> {
        let options = self.options;
        let mut line = Line {
            text: record,
            number,
            context: false,
        };
        self.stats.scanned += 1;
        let Some(pattern) = &options.pattern else {
            return output.emit(line);
        };
        let (content, ending) = split_line(&line.text);
        let matched = pattern.is_match(content);
        self.stats.matched += usize::from(matched);
        if options.only {
            let content = content.to_owned();
            return self.only(&content, |_| number, output);
        }
        if options.passthru {
            line.context = !(matched && self.matches_left > 0);
            if !line.context {
                self.matches_left -= 1;
                if let Some(replacement) = &options.replacement {
                    let mut rewritten = expand_all(options, content, replacement);
                    rewritten.push_str(ending);
                    line.text = rewritten;
                }
            }
            return output.emit(line);
        }
        if let Some(replacement) = &options.replacement
            && matched
            && !options.invert
        {
            let mut rewritten = expand_all(options, content, replacement);
            rewritten.push_str(ending);
            line.text = rewritten;
        }
        self.select(line, matched, output)
    }

    fn only(
        &mut self,
        text: &str,
        number_at: impl Fn(usize) -> usize,
        output: &mut Output<impl Write>,
    ) -> io::Result<bool> {
        let options = self.options;
        let pattern = options.pattern.as_ref().expect("-o requires a pattern");
        for captures in pattern.captures_iter(text).take(self.matches_left) {
            self.matches_left -= 1;
            let mut expanded = String::new();
            match &options.replacement {
                Some(replacement) if options.fixed => {
                    expanded.push_str(replacement);
                }
                Some(replacement) => {
                    captures.expand(replacement, &mut expanded);
                }
                None => expanded.push_str(&captures[0]),
            }
            let first = number_at(captures.get(0).map_or(0, |m| m.start()));
            let body = expanded.strip_suffix('\n').unwrap_or(&expanded);
            for (index, part) in body.split('\n').enumerate() {
                if !output.emit(Line {
                    text: format!("{part}\n"),
                    number: first + index,
                    context: false,
                })? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    // Applies -v, -m and -C to one line whose match result is known.
    fn select(
        &mut self,
        mut line: Line,
        matched: bool,
        output: &mut Output<impl Write>,
    ) -> io::Result<bool> {
        let options = self.options;
        if matched != options.invert && self.matches_left > 0 {
            self.matches_left -= 1;
            self.after = options.context;
            while let Some(previous) = self.before.pop_front() {
                if !output.emit(previous)? {
                    return Ok(false);
                }
            }
            return output.emit(line);
        }
        line.context = true;
        if self.after > 0 {
            self.after -= 1;
            return output.emit(line);
        }
        if options.context > 0 {
            if self.before.len() == options.context {
                self.before.pop_front();
            }
            self.before.push_back(line);
        }
        Ok(true)
    }

    // -U: search the selected lines as one text, like rg --multiline.
    fn finish_block(
        &mut self,
        output: &mut Output<impl Write>,
    ) -> io::Result<bool> {
        let options = self.options;
        let records = std::mem::take(&mut self.block);
        if records.is_empty() {
            return Ok(true);
        }
        let pattern = options.pattern.as_ref().expect("-U requires a pattern");
        let mut text = String::new();
        let mut starts = Vec::with_capacity(records.len());
        for record in &records {
            starts.push(text.len());
            text.push_str(&record.text);
        }
        let line_of = |offset: usize| {
            starts.partition_point(|&start| start <= offset).max(1) - 1
        };
        self.stats.scanned += records.len();
        if options.only {
            let mut matched = vec![false; records.len()];
            for found in pattern.find_iter(&text) {
                let last = line_of(found.end().max(found.start() + 1) - 1);
                matched[line_of(found.start())..=last].fill(true);
            }
            self.stats.matched += matched.iter().filter(|&&m| m).count();
            return self.only(&text, |at| records[line_of(at)].number, output);
        }
        // Matches touching the same lines form one group; -m counts groups.
        let limit = if options.invert {
            usize::MAX
        } else {
            std::mem::replace(&mut self.matches_left, usize::MAX)
        };
        let mut groups: Vec<(usize, usize, Vec<regex::Captures>)> = Vec::new();
        for captures in pattern.captures_iter(&text) {
            let found = captures.get(0).expect("group 0 always matches");
            let first = line_of(found.start());
            let last = line_of(found.end().max(found.start() + 1) - 1);
            let full = groups.len() == limit;
            match groups.last_mut() {
                Some(group) if first <= group.1 => {
                    group.1 = group.1.max(last);
                    group.2.push(captures);
                }
                _ if full => break,
                _ => groups.push((first, last, vec![captures])),
            }
        }
        self.stats.matched += groups
            .iter()
            .map(|(first, last, _)| last - first + 1)
            .sum::<usize>();
        if let Some(replacement) = &options.replacement
            && !options.invert
        {
            let mut cursor = 0;
            for (first, last, matches) in groups {
                if options.passthru {
                    for record in &records[cursor..first] {
                        if !output.emit(Line {
                            text: record.text.clone(),
                            number: record.number,
                            context: true,
                        })? {
                            return Ok(false);
                        }
                    }
                }
                cursor = last + 1;
                let end = starts.get(last + 1).copied().unwrap_or(text.len());
                let mut position = starts[first];
                let mut rewritten = String::new();
                for captures in matches {
                    let found =
                        captures.get(0).expect("group 0 always matches");
                    rewritten.push_str(&text[position..found.start()]);
                    if options.fixed {
                        rewritten.push_str(replacement);
                    } else {
                        captures.expand(replacement, &mut rewritten);
                    }
                    position = found.end();
                }
                rewritten.push_str(&text[position..end]);
                if !output.emit(Line {
                    text: rewritten,
                    number: records[first].number,
                    context: false,
                })? {
                    return Ok(false);
                }
            }
            if options.passthru {
                for mut record in records.into_iter().skip(cursor) {
                    record.context = true;
                    if !output.emit(record)? {
                        return Ok(false);
                    }
                }
            }
            return Ok(true);
        }
        let mut matched = vec![false; records.len()];
        for (first, last, _) in &groups {
            matched[*first..=*last].fill(true);
        }
        let mut previous = 0;
        for (mut record, matched) in records.into_iter().zip(matched) {
            if options.passthru {
                record.context = !matched;
                if !output.emit(record)? {
                    return Ok(false);
                }
                continue;
            }
            if previous + 1 < record.number {
                self.gap();
            }
            previous = record.number;
            if !self.select(record, matched, output)? {
                return Ok(false);
            }
            if self.matches_left == 0 && self.after == 0 {
                break;
            }
        }
        Ok(true)
    }
}

fn read_line(
    input: &mut impl BufRead,
    bytes: &mut Vec<u8>,
    stats: &mut Stats,
) -> io::Result<bool> {
    bytes.clear();
    if input.read_until(b'\n', bytes)? == 0 {
        stats.eof = true;
        return Ok(false);
    }
    stats.read += 1;
    Ok(true)
}

fn resolve(line: i64, total: i128) -> i128 {
    if line < 0 {
        total + i128::from(line) + 1
    } else {
        i128::from(line)
    }
}

fn select(
    options: &Options,
    mut input: impl BufRead,
    filter: &mut Filter<'_>,
    output: &mut Output<impl Write>,
) -> io::Result<bool> {
    if options.limit == Some(0) {
        return Ok(false);
    }
    if filter.selection_complete() {
        return Ok(true);
    }
    let mut bytes = Vec::new();
    let mut buffered = if options
        .ranges
        .iter()
        .any(|range| range.from < 0 || range.to.is_some_and(|to| to < 0))
    {
        let mut records = Vec::new();
        while read_line(&mut input, &mut bytes, &mut filter.stats)? {
            records.push(String::from_utf8_lossy(&bytes).into_owned());
        }
        Some(records.into_iter())
    } else {
        None
    };
    let total = buffered
        .as_ref()
        .map_or(i128::MAX, |records| records.len() as i128);
    let mut ranges: Vec<_> = options
        .ranges
        .iter()
        .filter_map(|range| {
            let from = resolve(range.from, total).max(1);
            let to = range.to.map_or(total, |to| resolve(to, total).min(total));
            (from <= to).then_some((from, to))
        })
        .collect();
    ranges.sort_unstable();
    let mut ranges = ranges.into_iter().peekable();
    let mut previous = 0;
    for number in 1.. {
        let line = number as i128;
        while ranges.peek().is_some_and(|&(_, to)| to < line) {
            ranges.next();
        }
        let Some(&(from, _)) = ranges.peek() else {
            break;
        };
        if previous + 1 < number {
            filter.gap();
        }
        if filter.selection_complete() {
            break;
        }
        let record = match &mut buffered {
            Some(records) => records.next(),
            None if read_line(&mut input, &mut bytes, &mut filter.stats)? => {
                (line >= from)
                    .then(|| String::from_utf8_lossy(&bytes).into_owned())
            }
            None => break,
        };
        if line < from {
            continue;
        }
        previous = number;
        let record = record.expect("selected line is within buffered input");
        if !filter.accept(record, number, output)?
            || output.remaining == Some(0)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn run(options: Options) -> io::Result<()> {
    let mut filter = Filter {
        options: &options,
        before: VecDeque::new(),
        after: 0,
        matches_left: options.max_matches.unwrap_or(usize::MAX),
        block: Vec::new(),
        stats: Stats::default(),
    };
    let mut output = Output {
        writer: io::stdout().lock(),
        sorted: options.sort.then(Vec::new),
        previous: None,
        uniq: options.uniq,
        remaining: options.limit,
        clipped: false,
        line_numbers: options.line_numbers,
        group_context: options.context > 0 && !options.sort,
        last_number: None,
        pending_gap: false,
    };
    let mut complete =
        select(&options, io::stdin().lock(), &mut filter, &mut output)?;
    if complete {
        complete = filter.finish_block(&mut output)?;
    }
    let complete = complete && output.finish()?;
    output.writer.flush()?;
    if options.stats {
        let Stats {
            read,
            scanned,
            matched,
            eof,
        } = filter.stats;
        let truncated = match (complete, output.clipped) {
            (true, _) => "false",
            (false, true) => "true",
            (false, false) => "unknown",
        };
        eprintln!(
            "lsel: lines_read={read} lines_scanned={scanned} lines_matched={matched} eof={eof} truncated={truncated}"
        );
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let result = match parse() {
        Ok(Some(_)) if io::stdin().is_terminal() => {
            let end = HELP
                .find("\n\n")
                .expect("help summary ends with a blank line");
            writeln!(io::stdout().lock(), "{}", &HELP[..end])
        }
        Ok(Some(options)) => run(options),
        Ok(None) => io::stdout().lock().write_all(HELP.as_bytes()),
        Err(error) => {
            eprintln!("lsel: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("lsel: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

// Expands rg-style forms: -C1, -in, -m1 and --context=1.
fn split_args(
    args: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Vec<String>, String> {
    const SHORT_VALUE: &str = "elrCLm";
    const LONG_VALUE: &[&str] = &[
        "regexp",
        "lines",
        "replace",
        "context",
        "limit",
        "max-count",
    ];
    let mut out = Vec::new();
    let mut args = args.map(|arg| {
        arg.into_string()
            .map_err(|_| "arguments must be UTF-8".to_owned())
    });
    while let Some(arg) = args.next() {
        let arg = arg?;
        if arg == "--" {
            out.push(arg);
            for rest in args.by_ref() {
                out.push(rest?);
            }
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            match long.split_once('=') {
                Some((name, value)) => {
                    out.push(format!("--{name}"));
                    out.push(value.to_owned());
                }
                None => {
                    let takes_value = LONG_VALUE.contains(&long);
                    out.push(arg);
                    if takes_value && let Some(value) = args.next() {
                        out.push(value?);
                    }
                }
            }
            continue;
        }
        let Some(short) = arg.strip_prefix('-').filter(|s| s.len() > 1) else {
            out.push(arg);
            continue;
        };
        for (index, flag) in short.char_indices() {
            out.push(format!("-{flag}"));
            if SHORT_VALUE.contains(flag) {
                let rest = &short[index + flag.len_utf8()..];
                if !rest.is_empty() {
                    out.push(rest.to_owned());
                } else if let Some(value) = args.next() {
                    out.push(value?);
                }
                break;
            }
        }
    }
    Ok(out)
}

fn expand_all(options: &Options, text: &str, replacement: &str) -> String {
    let pattern = options.pattern.as_ref().expect("-r requires a pattern");
    if options.fixed {
        pattern.replace_all(text, NoExpand(replacement))
    } else {
        pattern.replace_all(text, replacement)
    }
    .into_owned()
}
