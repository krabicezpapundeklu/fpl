use std::{
    io::{stdout, Result},
    path::{Path, PathBuf},
};

use clap::Parser;
use csv::{ReaderBuilder, WriterBuilder};
use html_escape::encode_text;

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alpha1, anychar, char, digit1, one_of, space0},
    combinator::{fail, opt, verify},
    error::Error,
    multi::many_till,
    IResult,
};

use serde::Deserialize;

#[derive(Parser)]
struct Args {
    input: PathBuf,

    #[arg(long)]
    html: bool,

    #[arg(long)]
    unique: bool,
}

#[derive(Deserialize)]
struct Record {
    id: i64,
    text: String,
}

fn alphas(count: usize, s: &str) -> IResult<&str, &str> {
    verify(alpha1, |s: &str| s.len() == count)(s)
}

fn fpl(s: &str) -> IResult<&str, &str> {
    if let Ok((s, fpl)) = tag::<&str, &str, Error<&str>>("fpl")(s) {
        return Ok((s, fpl));
    }

    let start = s;

    let (s, _) = tag("full")(s)?;
    let (s, _) = opt_one_of(" -", s)?;

    let (s, _) = alt((
        tag("peformance"),
        tag("perf."),
        tag("perfformance"),
        tag("performance"),
        tag("performane"),
        tag("perfromance"),
        tag("perormance"),
    ))(s)?;

    let (s, _) = space0(s)?;
    let (s, _) = opt(tag("level"))(s)?;

    Ok((s, &start[0..start.len() - s.len()]))
}

fn fpl_grade(s: &str) -> IResult<&str, &str> {
    let (s, _) = fpl(s)?;
    let (s, _) = space0(s)?;

    let (s, _) = opt(alt((
        tag("-"),
        tag(","),
        tag(":"),
        tag("("),
        tag("="),
        tag("at grade level"),
        tag("at"),
        tag("for this pd is"),
        tag("for this position is"),
        tag("is at the"),
        tag("is at"),
        tag("is level:"),
        tag("is the"),
        tag("is"),
        tag("of position is"),
        tag("of position:"),
        tag("of the position is"),
        tag("of this pd is"),
        tag("of this position is"),
    )))(s)?;

    let (s, _) = space0(s)?;

    grade(s)
}

fn get_fpl_grade(s: &str) -> Option<&str> {
    if let Ok((_, (_, grade))) = many_till(anychar, fpl_grade)(s) {
        Some(grade)
    } else {
        None
    }
}

fn get_match_prefix_and_suffix<'a>(s: &'a str, m: &'a str) -> (&'a str, &'a str) {
    unsafe {
        let start = s.as_ptr();
        let match_start = m.as_ptr();
        let offset = match_start.offset_from(start).unsigned_abs();

        let prefix = &s[0..offset];
        let suffix = &s[offset + m.len()..];

        (prefix, suffix)
    }
}

fn grade(s: &str) -> IResult<&str, &str> {
    if let Ok((s, grade)) = max_digits(2, s) {
        return Ok((s, grade));
    }

    let (s, _) = alphas(2, s)?;
    let (s, sep) = opt_one_of(" -.", s)?;

    match sep {
        None | Some(' ') => max_digits(2, s),
        Some(sep) => {
            let (s, grade_or_series) = max_digits(4, s)?;

            if let Ok((s, _)) = char::<&str, Error<&str>>(sep)(s) {
                if let Ok((s, grade)) = max_digits(2, s) {
                    return Ok((s, grade));
                }
            }

            if grade_or_series.len() <= 2 {
                Ok((s, grade_or_series))
            } else {
                fail(s)
            }
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let records = read_records(args.input)?;

    if args.unique {
        print_unique_texts(&records, args.html);
    } else {
        print_records_with_grades(&records)?;
    }

    Ok(())
}

fn max_digits(count: usize, s: &str) -> IResult<&str, &str> {
    verify(digit1, |s: &str| s.len() <= count)(s)
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn opt_one_of<'a>(list: &str, s: &'a str) -> IResult<&'a str, Option<char>> {
    opt(one_of(list))(s)
}

fn print_records_with_grades(records: &[Record]) -> Result<()> {
    let mut writer = WriterBuilder::new().from_writer(stdout());

    for record in records {
        let text = normalize(&record.text);
        let grade = get_fpl_grade(&text).unwrap_or_default();

        writer.write_record([record.id.to_string().as_str(), grade, &record.text])?;
    }

    Ok(())
}

fn print_unique_texts(records: &[Record], html: bool) {
    let mut unique_texts: Vec<_> = records
        .iter()
        .map(|record| normalize(&record.text))
        .collect();

    unique_texts.sort();
    unique_texts.dedup();

    if html {
        println!("<!doctype html>");
        println!("<html lang='en'>");
        println!("\t<body>");
        println!("\t<style>");
        println!("\t.fpl {{color: red}}");
        println!("\ttable, td, th {{border: 1px solid; border-collapse: collapse}}");
        println!("\t</style>");
        println!("\t\t<table>");
        println!("\t\t\t<thead>");
        println!("\t\t\t\t<tr>");
        println!("\t\t\t\t\t<th scope='col'>Line</th>");
        println!("\t\t\t\t\t<th scope='col'>Grade</th>");
        println!("\t\t\t\t\t<th scope='col'>Text</th>");
        println!("\t\t\t\t</tr>");
        println!("\t\t\t</thead>");
        println!("\t\t\t<tbody>");

        for (line, text) in unique_texts.iter().enumerate() {
            println!("\t\t\t\t<tr>");
            println!("\t\t\t\t\t<td>{}</td>", line + 1);

            if let Some(grade) = get_fpl_grade(text) {
                println!("\t\t\t\t\t<td>{grade}</td>");

                let (prefix, suffix) = get_match_prefix_and_suffix(text, grade);

                println!(
                    "\t\t\t\t\t<td>{}<span class='fpl'>{}</span>{}</td>",
                    encode_text(prefix),
                    encode_text(grade),
                    encode_text(suffix)
                );
            } else {
                println!("\t\t\t\t\t<td></td>");
                println!("\t\t\t\t\t<td>{}</td>", encode_text(text));
            }

            println!("\t\t\t\t</tr>");
        }

        println!("\t\t\t</tbody>");
        println!("\t\t</table>");
        println!("\t</body>");
        println!("</html>");
    } else {
        for text in unique_texts {
            if let Some(grade) = get_fpl_grade(&text) {
                print!("{grade}");
            }

            println!("|{text}");
        }
    }
}

fn read_records<P>(path: P) -> Result<Vec<Record>>
where
    P: AsRef<Path>,
{
    let mut csv = ReaderBuilder::new().has_headers(false).from_path(path)?;
    let mut records = Vec::new();

    for record in csv.deserialize() {
        records.push(record?);
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_fpl() {
        assert_eq!(fpl("fpl"), Ok(("", "fpl")));
        assert_eq!(fpl("full peformance"), Ok(("", "full peformance")));
        assert_eq!(fpl("full perf."), Ok(("", "full perf.")));
        assert_eq!(fpl("full perfformance"), Ok(("", "full perfformance")));
        assert_eq!(fpl("full performance"), Ok(("", "full performance")));
        assert_eq!(fpl("full performane"), Ok(("", "full performane")));
        assert_eq!(fpl("full perfromance "), Ok(("", "full perfromance ")));
        assert_eq!(fpl("full-performance"), Ok(("", "full-performance")));

        assert_eq!(
            fpl("full performance level"),
            Ok(("", "full performance level"))
        );

        assert_eq!(
            fpl("full perormance level"),
            Ok(("", "full perormance level"))
        );

        assert_eq!(
            fpl("fullperformance level"),
            Ok(("", "fullperformance level"))
        );
    }

    #[test]
    fn test_grade() {
        assert_eq!(grade("1"), Ok(("", "1")));
        assert_eq!(grade("12"), Ok(("", "12")));
        assert_eq!(grade("gs 11"), Ok(("", "11")));
        assert_eq!(grade("gs-0510-09"), Ok(("", "09")));
        assert_eq!(grade("gs-0998-6"), Ok(("", "6")));
        assert_eq!(grade("gs-13"), Ok(("", "13")));
        assert_eq!(grade("gs-13.xxx"), Ok((".xxx", "13")));
        assert_eq!(grade("gs-13-"), Ok(("-", "13")));
        assert_eq!(grade("gs-201-13"), Ok(("", "13")));
        assert_eq!(grade("gs-7"), Ok(("", "7")));
        assert_eq!(grade("gs15"), Ok(("", "15")));
        assert_eq!(grade("gs7"), Ok(("", "7")));
        assert_eq!(grade("wg 7"), Ok(("", "7")));
        assert_eq!(grade("wg-08"), Ok(("", "08")));
        assert_eq!(grade("wl-08"), Ok(("", "08")));
        assert_eq!(grade("ws-7"), Ok(("", "7")));
        assert_eq!(grade("gs.0343.18"), Ok(("", "18")));

        assert!(grade("123").is_err());
        assert!(grade("gs 123").is_err());
        assert!(grade("gs-123").is_err());
        assert!(grade("gs-1234-").is_err());
        assert!(grade("gs-1234-123").is_err());
        assert!(grade("gs-12345-12").is_err());
        assert!(grade("gs123").is_err());
    }

    #[test]
    fn test_normalize() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("\n\nabc   \t  DEF 1\n2\t3\n  "), "abc def 1 2 3");
    }
}
