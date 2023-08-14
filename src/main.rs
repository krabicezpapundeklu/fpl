use std::{
    io::{stdout, Result},
    path::{Path, PathBuf},
};

use clap::Parser;
use csv::{ReaderBuilder, WriterBuilder};
use html_escape::encode_text;

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case},
    character::complete::{alpha1, anychar, char, digit1, multispace0, one_of},
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
    id: usize,
    text: String,
}

fn alphas(count: usize, s: &str) -> IResult<&str, &str> {
    verify(alpha1, |s: &str| s.len() == count)(s)
}

fn dedup_records(records: &mut Vec<Record>) {
    records.iter_mut().for_each(|r| r.text = normalize(&r.text));
    records.sort_by(|a, b| a.text.cmp(&b.text));
    records.dedup_by(|a, b| a.text == b.text);
}

fn fpl(s: &str) -> IResult<&str, &str> {
    if let Ok((s, fpl)) = tag_no_case::<&str, &str, Error<&str>>("fpl")(s) {
        return Ok((s, fpl));
    }

    let start = s;

    let (s, _) = alt((tag_no_case("full"), tag_no_case("poll")))(s)?;
    let (s, _) = opt_one_of(" -", s)?;

    let (s, _) = alt((
        words(&["career", "ladder", "grade"]),
        tag_no_case("grade"),
        tag_no_case("peformance"),
        tag_no_case("perf."),
        tag_no_case("perfformance"),
        tag_no_case("performance"),
        tag_no_case("performane"),
        tag_no_case("perfromance"),
        tag_no_case("perormance"),
        tag_no_case("promotion"),
    ))(s)?;

    let (s, _) = multispace0(s)?;
    let (s, _) = opt(tag_no_case("level"))(s)?;

    Ok((s, &start[0..start.len() - s.len()]))
}

fn fpl_grade(s: &str) -> IResult<&str, &str> {
    let (s, _) = fpl(s)?;
    let (s, _) = multispace0(s)?;

    let (s, _) = opt(alt((
        alt((
            tag("-"),
            tag(","),
            tag(":"),
            tag("(fpl)"),
            tag("("),
            tag("="),
        )),
        words(&["at", "grade", "level"]),
        tag_no_case("at"),
        words(&["for", "this", "pd", "is"]),
        words(&["for", "this", "position", "is"]),
        words(&["is", "at", "the"]),
        words(&["is", "at"]),
        words(&["is", "level", ":"]),
        words(&["is", "the"]),
        words(&["management", "analyst"]),
        tag_no_case("is"),
        words(&["of", "a", "career", "ladder", "position"]),
        words(&["of", "a"]),
        words(&["of", "position", "is"]),
        words(&["of", "position", ":"]),
        words(&["of", "the", "position", "is"]),
        words(&["of", "this", "pd", "is"]),
        words(&["of", "this", "position", "is"]),
    )))(s)?;

    let (s, _) = multispace0(s)?;

    max_grade(s)
}

fn get_fpl_grade(s: &str) -> Option<&str> {
    if let Ok((_, (_, grade))) = many_till(anychar, fpl_grade)(s) {
        Some(grade)
    } else if let Ok((_, (_, grade))) = many_till(anychar, target_grade)(s) {
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
    let (s, _) = opt(tag(" "))(s)?;

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
    let mut records = read_records(args.input)?;

    if args.unique {
        dedup_records(&mut records);
    }

    if args.html {
        print_html(&records, !args.unique);
    } else {
        print_csv(&records, !args.unique)?;
    }

    Ok(())
}

fn max_digits(count: usize, s: &str) -> IResult<&str, &str> {
    verify(digit1, |s: &str| s.len() <= count)(s)
}

fn max_grade(s: &str) -> IResult<&str, &str> {
    let (mut s, mut max_grade) = grade(s)?;

    loop {
        (s, _) = multispace0(s)?;
        (s, _) = opt_one_of(",/", s)?;
        (s, _) = multispace0(s)?;

        if let Ok((gs, grade)) = grade(s) {
            (s, max_grade) = (gs, grade);
        } else {
            return Ok((s, max_grade));
        }
    }
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

fn print_csv(records: &[Record], print_ids: bool) -> Result<()> {
    let mut writer = WriterBuilder::new().from_writer(stdout());

    for record in records {
        let grade = get_fpl_grade(&record.text).unwrap_or_default();

        if print_ids {
            writer.write_record([record.id.to_string().as_str(), grade, &record.text])?;
        } else {
            writer.write_record([grade, &record.text])?;
        }
    }

    Ok(())
}

fn print_html(records: &[Record], print_ids: bool) {
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

    println!(
        "\t\t\t\t\t<th scope='col'>{}</th>",
        if print_ids { "ID" } else { "Line" }
    );

    println!("\t\t\t\t\t<th scope='col'>Grade</th>");
    println!("\t\t\t\t\t<th scope='col'>Text</th>");
    println!("\t\t\t\t</tr>");
    println!("\t\t\t</thead>");
    println!("\t\t\t<tbody>");

    for (i, record) in records.iter().enumerate() {
        println!("\t\t\t\t<tr>");

        println!(
            "\t\t\t\t\t<td>{}</td>",
            if print_ids { record.id } else { i + 1 }
        );

        if let Some(grade) = get_fpl_grade(&record.text) {
            println!("\t\t\t\t\t<td>{grade}</td>");

            let (prefix, suffix) = get_match_prefix_and_suffix(&record.text, grade);

            println!(
                "\t\t\t\t\t<td>{}<span class='fpl'>{}</span>{}</td>",
                encode_text(prefix),
                encode_text(grade),
                encode_text(suffix)
            );
        } else {
            println!("\t\t\t\t\t<td></td>");
            println!("\t\t\t\t\t<td>{}</td>", encode_text(&record.text));
        }

        println!("\t\t\t\t</tr>");
    }

    println!("\t\t\t</tbody>");
    println!("\t\t</table>");
    println!("\t</body>");
    println!("</html>");
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

fn target_grade(s: &str) -> IResult<&str, &str> {
    let (s, _) = tag_no_case("target")(s)?;
    let (s, _) = opt(tag_no_case("ed"))(s)?;
    let (s, _) = multispace0(s)?;

    let (s, _) = opt(alt((
        tag_no_case("to"),
        words(&["position", ","]),
        words(&["position", "posted", "as", "at", "a"]),
    )))(s)?;

    let (s, _) = multispace0(s)?;

    max_grade(s)
}

fn words(words: &'static [&str]) -> impl FnMut(&str) -> IResult<&str, &str> {
    move |s| {
        let mut i = s;

        for word in words {
            i = multispace0(i)?.0;
            i = tag_no_case(*word)(i)?.0;
        }

        Ok((i, &s[0..s.len() - s.len()]))
    }
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
        assert_eq!(grade("gs- 13"), Ok(("", "13")));
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
    fn test_max_grade() {
        assert_eq!(max_grade("gs-11/12/13"), Ok(("", "13")));
        assert_eq!(max_grade("gs-5 / gs-6 / gs-7"), Ok(("", "7")));
    }

    #[test]
    fn test_normalize() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("\n\nabc   \t  DEF 1\n2\t3\n  "), "abc def 1 2 3");
    }
}
