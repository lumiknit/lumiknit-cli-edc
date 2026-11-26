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
    stats: bool,
    line_numbers: bool,
}

fn parse() -> Result<Option<Options>, String> {
    let mut options = Options {
        ranges: vec![LineRange { from: 1, to: None }],
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
        stats: true,
        line_numbers: io::stdout().is_terminal(),
    };
    let mut args = std::env::args_os().skip(1);
    let mut pattern = None;
    let mut ignore_case = false;
    let mut has_context = false;
    while let Some(arg) = args.next() {
        let arg = arg.to_str().ok_or("options must be UTF-8")?;
        let mut value = || {
            args.next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| format!("{arg} requires a UTF-8 value"))
        };
        match arg {
            "-h" | "--help" => return Ok(None),
            "-l" | "--lines" => options.ranges = line_ranges(&value()?)?,
            "-m" | "--match" => pattern = Some(value()?),
            "-r" | "--replace" => options.replacement = Some(value()?),
            "-I" | "--ignore-case" => ignore_case = true,
            "-F" | "--fixed-strings" => options.fixed = true,
            "-v" | "--invert-match" => options.invert = true,
            "-o" | "--only-matching" => options.only = true,
            "-C" | "--context" => {
                has_context = true;
                options.context = count(&value()?)?;
            }
            "-S" | "--sort" => options.sort = true,
            "-U" | "--uniq" => options.uniq = true,
            "-L" | "--limit" => options.limit = Some(count(&value()?)?),
            "-M" | "--max-count" => {
                options.max_matches = Some(count(&value()?)?)
            }
            "-X" | "--multiline" => options.multiline = true,
            "-n" | "--line-number" => options.line_numbers = true,
            "-N" | "--no-line-number" => options.line_numbers = false,
            "--no-stats" => options.stats = false,
            _ => return Err(format!("unknown option {arg:?} (see --help)")),
        }
    }
    if pattern.is_none()
        && (ignore_case
            || options.fixed
            || options.only
            || options.invert
            || has_context
            || options.max_matches.is_some()
            || options.multiline
            || options.replacement.is_some())
    {
        return Err("-I, -F, -v, -C, -r, -o, -M and -X require -m".into());
    }
    if options.only && (options.invert || has_context) {
        return Err("-o cannot be combined with -v or -C".into());
    }
    if options.replacement.is_some() && (options.invert || has_context) {
        return Err("-r cannot be combined with -v or -C".into());
    }
    if options.multiline && has_context {
        return Err("-X cannot be combined with -C".into());
    }
    if let Some(pattern) = pattern {
        let pattern = if options.fixed {
            regex::escape(&pattern)
        } else {
            pattern
        };
        options.pattern = Some(
            RegexBuilder::new(&pattern)
                .case_insensitive(ignore_case)
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
            let sign_length = usize::from(range.starts_with('-'));
            let (from, to) = match range[sign_length..].find('-') {
                Some(separator) => {
                    let (from, end) = range.split_at(sign_length + separator);
                    let to = match &end[1..] {
                        "" => None,
                        end => Some(line_number(end)?),
                    };
                    (line_number(from)?, to)
                }
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
    block: String,
    block_start: usize,
    stats: Stats,
}

impl Filter<'_> {
    fn selection_complete(&self) -> bool {
        self.matches_left == 0
            && self.after == 0
            && (self.options.replacement.is_none() || self.options.only)
    }
    fn accept(
        &mut self,
        record: String,
        number: usize,
        output: &mut Output<impl Write>,
    ) -> io::Result<bool> {
        if self.options.multiline {
            if self.block_start == 0 {
                self.block_start = number;
            }
            self.block.push_str(&record);
            return Ok(true);
        }
        self.line(record, number, output)
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
        let line_count = if options.multiline {
            line.text.split_inclusive('\n').count()
        } else {
            1
        };
        self.stats.scanned += line_count;
        let Some(pattern) = &options.pattern else {
            return output.emit(line);
        };
        let (content, ending) = if options.multiline {
            (line.text.as_str(), "")
        } else {
            split_line(&line.text)
        };
        let matched = pattern.is_match(content);
        self.stats.matched += usize::from(matched) * line_count;
        if options.only {
            for captures in
                pattern.captures_iter(content).take(self.matches_left)
            {
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
                expanded.push('\n');
                if !output.emit(Line {
                    text: expanded,
                    number,
                    context: false,
                })? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        if let Some(replacement) = &options.replacement {
            if matched && self.matches_left > 0 {
                let limit = if options.max_matches.is_some() {
                    let count = pattern
                        .find_iter(content)
                        .take(self.matches_left)
                        .count();
                    self.matches_left -= count;
                    count
                } else {
                    0
                };
                let mut rewritten = if options.fixed {
                    pattern.replacen(content, limit, NoExpand(replacement))
                } else {
                    pattern.replacen(content, limit, replacement.as_str())
                }
                .into_owned();
                rewritten.push_str(ending);
                line.text = rewritten;
                return output.emit(line);
            }
            return output.emit(line);
        }
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
            filter.before.clear();
            filter.after = 0;
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
        block: String::new(),
        block_start: 0,
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
    if complete && filter.block_start != 0 {
        let block = std::mem::take(&mut filter.block);
        complete = filter.line(block, filter.block_start, &mut output)?;
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
