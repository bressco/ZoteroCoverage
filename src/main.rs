use anyhow::Context;
use clap::{Parser, ValueEnum};
use clap_file::Input;
use pandoc_ast::{Block, Caption, Inline, MetaValue, Pandoc, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::{env, fmt};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
enum OutputFormat {
    Json,
    Text,
    Pr,
    Gha
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // Path to a document as pandoc JSON AST; `-` (or omitted) reads from stdin
    #[clap(short, long, default_value = "-")]
    document: Input,

    // Path to a Zotero library, CSL JSON format; optional if the document
    // metadata contains a `bibliography` entry
    #[clap(short, long)]
    zotero_lib: Option<Input>,

    //enable verbose output
    #[clap(short, long)]
    verbose: bool,

    //output format: json, text or pr (default: text)
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Deserialize, Debug, Clone, Hash, PartialEq, Serialize)]
struct Citations {
    #[serde(rename = "citation-key")]
    pub citation_key: String,
}

impl fmt::Display for Citations {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.citation_key)
    }
}

#[derive(Serialize)]
struct Report {
    uncited: Vec<Citations>,
    total_bibliography: usize,
    total_citations: usize,
    all_cited: bool
}

fn format_pr_comment(report: &Report) -> String {
    let mut out = String::from("<!-- zoterocoverage -->\n");
    out.push_str(&format!(
        "## {} ZoteroCoverage: ",
        if report.all_cited { "[Pass]" } else { "[Fail]" }
    ));
    if report.all_cited {
        out.push_str(&format!("All {} sources cited\n", report.total_bibliography));
    } else {
        out.push_str(&format!(
            "{} of {} sources not cited\n\n",
            report.uncited.len(),
            report.total_bibliography
        ));
        out.push_str("The following sources in the bibliography are not cited in the document:\n");
        for d in &report.uncited {
            out.push_str(&format!("- `@{}`\n", d));
        }
    }
    out
}

fn format_gha(report: &Report) -> String {
    let timestamp =
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    let mut out = format!("report<<ZC_REPORT_EOF_{}\n", timestamp);
    out.push_str(format_pr_comment(report).as_str());
    out.push_str(format!("ZC_REPORT_EOF_{}", timestamp).as_str());

    out
}

fn format_output(report: &Report, format: &OutputFormat) -> anyhow::Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Text => {
            if report.all_cited {
                Ok("All sources cited".to_string())
            } else {
                let mut out = format!("{} Sources not cited:", report.uncited.len());
                for d in &report.uncited {
                    out.push_str(&format!("\n{}", d));
                }
                Ok(out)
            }
        }
        OutputFormat::Pr => Ok(format_pr_comment(report)),
        OutputFormat::Gha => Ok(format_gha(report))
    }
}

fn get_citations_bibliography(
    bibliography: &str,
) -> anyhow::Result<Vec<Citations>> {
    let v: Vec<Citations> = serde_json::from_str(bibliography)?;
    Ok(v)
}

fn get_citations_document(ast: &Pandoc) -> Vec<String> {
    let mut citations: Vec<String> = Vec::new();
    collect_cites_blocks(&ast.blocks, &mut citations);
    citations
}

fn collect_cites_blocks(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Plain(inlines) | Block::Para(inlines) => {
                collect_cites_inlines(inlines, out)
            }
            Block::LineBlock(lines) => collect_cites_inlines(&lines.concat(), out),
            Block::Header(_, _, inlines) => collect_cites_inlines(inlines, out),
            Block::BlockQuote(blocks) => collect_cites_blocks(blocks, out),
            Block::OrderedList(_, items) => {
                for item in items {
                    collect_cites_blocks(item, out);
                }
            }
            Block::BulletList(items) => {
                for item in items {
                    collect_cites_blocks(item, out);
                }
            }
            Block::DefinitionList(items) => {
                for (term, definitions) in items {
                    collect_cites_inlines(term, out);
                    for definition in definitions {
                        collect_cites_blocks(definition, out);
                    }
                }
            }
            Block::Figure(_, caption, blocks) => {
                collect_cites_caption(caption, out);
                collect_cites_blocks(blocks, out);
            }
            Block::Table(_, caption, _, head, bodies, foot) => {
                collect_cites_caption(caption, out);
                collect_cites_rows(&head.1, out);
                for body in bodies {
                    collect_cites_rows(&body.2, out);
                    collect_cites_rows(&body.3, out);
                }
                collect_cites_rows(&foot.1, out);
            }
            Block::Div(_, blocks) => collect_cites_blocks(blocks, out),
            // CodeBlock, RawBlock, HorizontalRule and Null carry no citations
            _ => {}
        }
    }
}

fn collect_cites_caption(caption: &Caption, out: &mut Vec<String>) {
    if let Some(short) = &caption.0 {
        collect_cites_inlines(short, out);
    }
    collect_cites_blocks(&caption.1, out);
}

fn collect_cites_rows(rows: &[Row], out: &mut Vec<String>) {
    for (_attr, cells) in rows {
        for cell in cells {
            // Cell = (Attr, Alignment, RowSpan, ColSpan, Vec<Block>)
            collect_cites_blocks(&cell.4, out);
        }
    }
}

fn collect_cites_inlines(inlines: &[Inline], out: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Cite(citations, contents) => {
                for citation in citations {
                    out.push(citation.citationId.clone());
                }
                collect_cites_inlines(contents, out);
            }
            Inline::Note(blocks) => collect_cites_blocks(blocks, out),
            Inline::Emph(i)
            | Inline::Underline(i)
            | Inline::Strong(i)
            | Inline::Strikeout(i)
            | Inline::Superscript(i)
            | Inline::Subscript(i)
            | Inline::SmallCaps(i) => collect_cites_inlines(i, out),
            Inline::Quoted(_, i) => collect_cites_inlines(i, out),
            Inline::Link(_, i, _) | Inline::Image(_, i, _) => collect_cites_inlines(i, out),
            Inline::Span(_, i) => collect_cites_inlines(i, out),
            // Str, Code, Space, SoftBreak, LineBreak, Math and RawInline carry
            // no citations
            _ => {}
        }
    }
}

fn parse_document(document_json: &str) -> anyhow::Result<Pandoc> {
    let ast: Pandoc = serde_json::from_str(document_json).with_context(|| {
        "document is not a valid pandoc JSON AST; produce it with \
         `pandoc --to=json` and pipe it into `--document`"
    })?;
    Ok(ast)
}

fn get_citation_difference(
    document: &[String],
    json: Vec<Citations>,
) -> Vec<Citations> {
    let document_set: HashSet<&str> = document.iter().map(String::as_str).collect();

    json.iter()
        .filter(|citation| !document_set.contains(citation.citation_key.as_str()))
        .cloned()
        .collect()
}

fn get_bibliography_path(ast: &Pandoc) -> anyhow::Result<String> {
    // Extract the bibliography path from the document metadata
    let value = ast
        .meta
        .get("bibliography")
        .ok_or_else(|| anyhow::anyhow!("no `bibliography` entry in document metadata"))?;
    match value {
        MetaValue::MetaString(path) => Ok(path.clone()),
        MetaValue::MetaInlines(inlines) => {
            let mut path = String::new();
            for inline in inlines {
                match inline {
                    Inline::Str(s) => path.push_str(s),
                    Inline::Space => path.push(' '),
                    _ => {}
                }
            }
            Ok(path.trim().to_string())
        }
        _ => Err(anyhow::anyhow!(
            "`bibliography` metadata must be a path (string)"
        )),
    }
}

fn run(args: &Args) -> anyhow::Result<Report> {
    // Read in the pandoc JSON AST document (from stdin when `-`)
    let mut document_input = args.document.lock();
    let mut document_json: String = String::new();

    // Read the document into a string
    if args.verbose {
        let source = document_input
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "standard input".to_string());
        println!("Reading document AST from {source}");
    }
    document_input.read_to_string(&mut document_json)?;
    let ast = parse_document(&document_json)?;

    let mut bibliography_json: String = String::new();
    // Get bibliography either from CLI or from the document metadata
    match &args.zotero_lib {
        Some(zotero_lib) => {
            // If found, read in the json based on the CLI
            let mut zotero_input = zotero_lib.lock();

            if args.verbose {
                let source = zotero_input
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "standard input".to_string());
                println!("Reading bibliography from {source}");
            }

            zotero_input.read_to_string(&mut bibliography_json)?;
        }
        None => {
            // Get bibliography path from the document metadata
            let bibliography_path = get_bibliography_path(&ast)?;
            let bp = shellexpand::tilde(&bibliography_path);

            // read from path
            if args.verbose {
                println!(
                    "Reading bibliography from path in document, {}",
                    bp.as_ref()
                )
            }

            bibliography_json = fs::read_to_string(bp.into_owned())?
        }
    }

    let citations_bibliography = get_citations_bibliography(&bibliography_json)?;

    if args.verbose {
        println!(
            "Found {} sources in bibliography",
            citations_bibliography.len()
        );
    }

    let citations_document = get_citations_document(&ast);

    if args.verbose {
        println!(
            "Found {} citations in document",
            citations_document.len()
        );
    }

    if args.verbose {
        println!("Comparing sources in bibliography and cited sources");
    }

    let differences = get_citation_difference(&citations_document, citations_bibliography.clone());

    let mut report = Report {
        all_cited: differences.is_empty(),
        uncited: differences,
        total_bibliography: citations_bibliography.len(),
        total_citations: citations_document.len(),
    };

    // Deterministic order for stable output (e.g. PR comments)
    report
        .uncited
        .sort_by(|a, b| a.citation_key.cmp(&b.citation_key));

    Ok(report)
}

fn main() {
    let args = Args::parse();
    let output_format = args.format;

    // Exit codes:
    //   0 - every source in the bibliography is cited
    //   1 - sources are not cited (results are printed first)
    //   2 - runtime error
    let report = match run(&args) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("error: {err:#}");
            std::process::exit(2);
        }
    };

    let output = match format_output(&report, &output_format) {
        Ok(output) => output,
        Err(err) => {
            eprintln!("error: {err:#}");
            std::process::exit(2);
        }
    };

    match output_format {
        OutputFormat::Gha => {
            if let Ok(path) = env::var("GITHUB_OUTPUT") {
                let mut f = OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)
                    .unwrap();
                f.write_all(output.as_bytes()).unwrap();
            }
            //if let Ok(path) = env::var("GITHUB_STEP_SUMMARY") {
            //    let mut f = OpenOptions::new()
            //        .append(true)
            //        .create(true)
            //        .open(path)
            //        .unwrap();
            //    f.write_all(
            //        format_pr_comment(&report).as_bytes()
            //    ).unwrap();
            //}
            println!("{}", format_pr_comment(&report));
        }
        _ => println!("{output}")
    }

    if !report.all_cited {
        // Fail on command line
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Citations, OutputFormat, Report, format_output, get_bibliography_path,
        get_citation_difference, get_citations_bibliography, get_citations_document,
        parse_document,
    };
    use pandoc_ast::{Block, Citation, CitationMode, Inline, Map, MetaValue, Pandoc};

    fn citation(id: &str, mode: CitationMode) -> Citation {
        Citation {
            citationId: id.to_string(),
            citationPrefix: Vec::new(),
            citationSuffix: Vec::new(),
            citationMode: mode,
            citationNoteNum: 0,
            citationHash: 0,
        }
    }

    fn citation_with_suffix(id: &str, suffix: Vec<Inline>) -> Citation {
        Citation {
            citationId: id.to_string(),
            citationPrefix: Vec::new(),
            citationSuffix: suffix,
            citationMode: CitationMode::NormalCitation,
            citationNoteNum: 0,
            citationHash: 0,
        }
    }

    fn pandoc_with(meta: Map<String, MetaValue>, blocks: Vec<Block>) -> Pandoc {
        Pandoc {
            meta,
            blocks,
            pandoc_api_version: vec![1, 23],
        }
    }

    #[test]
    fn test_get_citations_bibliography() {
        let testdata_json = r#"
    [
  {
    "id": ".2024",
    "accessed": {
      "date-parts": [
        [
          "2025",
          1,
          29
        ]
      ]
    },
    "citation-key": ".2024",
    "container-title": "JuristenZeitung",
    "container-title-short": "JZ",
    "DOI": "10.1628/jz-2024-0306",
    "ISSN": "0022-6882",
    "issue": "22",
    "issued": {
      "date-parts": [
        [
          "2024"
        ]
      ]
    },
    "language": "de",
    "page": "1007",
    "source": "DOI.org (Crossref)",
    "title": "Potenzial und Grenzen eines Einsatzes von Large Language Models in der öffentlichen Verwaltung",
    "type": "article-journal",
    "URL": "https://www.mohrsiebeck.com/10.1628/jz-2024-0306",
    "volume": "79"
  },
  {
    "id": ".2024a",
    "accessed": {
      "date-parts": [
        [
          "2025",
          1,
          29
        ]
      ]
    },
    "citation-key": ".2024a",
    "container-title": "Archiv für die civilistische Praxis",
    "container-title-short": "AcP",
    "DOI": "10.1628/acp-2024-0020",
    "ISSN": "0003-8997",
    "issue": "4-5",
    "issued": {
      "date-parts": [
        [
          "2024"
        ]
      ]
    },
    "language": "de",
    "page": "477",
    "source": "DOI.org (Crossref)",
    "title": "Kryptowerte als Sachen",
    "type": "article-journal",
    "URL": "https://www.mohrsiebeck.com/10.1628/acp-2024-0020",
    "volume": "224"
  },
  {
    "id": "AGGelnhausen.2024",
    "authority": "AG Gelnhausen",
    "citation-key": "AGGelnhausen.2024",
    "genre": "Urt.",
    "issued": {
      "date-parts": [
        [
          "2024",
          3,
          4
        ]
      ]
    },
    "jurisdiction": "de",
    "number": "52 C 76/24",
    "title": "AG Gelnhausen, 04.03.2024 - 52 C 76/24",
    "type": "legal_case"
  },
  {
    "id": "Alexander.2024",
    "author": [
      {
        "family": "Alexander",
        "given": ""
      }
    ],
    "citation-key": "Alexander.2024",
    "container-title": "UWG",
    "edition": "42",
    "editor": [
      {
        "family": "Köhler",
        "given": ""
      },
      {
        "family": "Bornkamm",
        "given": ""
      },
      {
        "family": "Feddersen",
        "given": ""
      }
    ],
    "issued": {
      "date-parts": [
        [
          "2024"
        ]
      ]
    },
    "source": "beck-online",
    "title": "§ 2 GeschGehG",
    "type": "entry-encyclopedia"
  },
  {
    "id": "Alexander.2024a",
    "author": [
      {
        "family": "Alexander",
        "given": ""
      }
    ],
    "citation-key": "Alexander.2024a",
    "container-title": "UWG",
    "edition": "42",
    "editor": [
      {
        "family": "Köhler",
        "given": ""
      },
      {
        "family": "Bornkamm",
        "given": ""
      },
      {
        "family": "Feddersen",
        "given": ""
      }
    ],
    "issued": {
      "date-parts": [
        [
          "2024"
        ]
      ]
    },
    "source": "beck-online",
    "title": "§ 6 GeschGehG",
    "type": "entry-encyclopedia"
  }
  ]
 "#;
        let out = vec![
            Citations {
                citation_key: ".2024".to_string(),
            },
            Citations {
                citation_key: ".2024a".to_string(),
            },
            Citations {
                citation_key: "AGGelnhausen.2024".to_string(),
            },
            Citations {
                citation_key: "Alexander.2024".to_string(),
            },
            Citations {
                citation_key: "Alexander.2024a".to_string(),
            },
        ];
        assert_eq!(get_citations_bibliography(testdata_json).unwrap(), out);
    }

    #[test]
    fn test_get_citations_bibliography_missing_field() {
        let testdata_json = r#"[{}]"#; // Missing citation-key
        let result = get_citations_bibliography(testdata_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_citations_document() {
        let ast = pandoc_with(
            Map::new(),
            vec![
                Block::Para(vec![
                    Inline::Str("See ".to_string()),
                    Inline::Cite(
                        vec![citation_with_suffix(
                            "Bomhard.2024b",
                            vec![
                                Inline::Str("Rn.".to_string()),
                                Inline::Space,
                                Inline::Str("15".to_string()),
                            ],
                        )],
                        Vec::new(),
                    ),
                ]),
                Block::Para(vec![Inline::Cite(
                    vec![
                        citation("BGH.2024", CitationMode::NormalCitation),
                        citation("BGH.2010c", CitationMode::NormalCitation),
                        citation("LGHamburg.2024", CitationMode::NormalCitation),
                    ],
                    Vec::new(),
                )]),
                Block::Para(vec![
                    Inline::Cite(
                        vec![citation("Alexander.2024", CitationMode::AuthorInText)],
                        vec![
                            Inline::Str("Alexander".to_string()),
                            Inline::Space,
                            Inline::Str("2024".to_string()),
                        ],
                    ),
                    Inline::Str("; ".to_string()),
                    Inline::Cite(
                        vec![citation("Alexander.2024a", CitationMode::AuthorInText)],
                        vec![
                            Inline::Str("Alexander".to_string()),
                            Inline::Space,
                            Inline::Str("2024a".to_string()),
                        ],
                    ),
                ]),
                // Citations in footnotes are collected; citations in inline or
                // block code are not
                Block::Para(vec![
                    Inline::Code(
                        ("".to_string(), Vec::new(), Vec::new()),
                        "@Bomhard.2024b is not a citation".to_string(),
                    ),
                    Inline::Note(vec![Block::Para(vec![Inline::Cite(
                        vec![citation("Footnote.2024", CitationMode::NormalCitation)],
                        Vec::new(),
                    )])]),
                ]),
                Block::CodeBlock(
                    ("".to_string(), Vec::new(), Vec::new()),
                    "@BGH.2024 should be ignored".to_string(),
                ),
            ],
        );

        assert_eq!(
            get_citations_document(&ast),
            vec![
                "Bomhard.2024b",
                "BGH.2024",
                "BGH.2010c",
                "LGHamburg.2024",
                "Alexander.2024",
                "Alexander.2024a",
                "Footnote.2024",
            ]
        );
    }

    #[test]
    fn test_parse_document() {
        let json = r#"{
  "pandoc-api-version": [1, 23],
  "meta": {},
  "blocks": [
    {
      "t": "Para",
      "c": [
        {
          "t": "Cite",
          "c": [
            [
              {
                "citationId": "Bomhard.2024b",
                "citationPrefix": [],
                "citationSuffix": [],
                "citationMode": { "t": "NormalCitation" },
                "citationNoteNum": 0,
                "citationHash": 0
              },
              {
                "citationId": "BGH.2010c",
                "citationPrefix": [],
                "citationSuffix": [],
                "citationMode": { "t": "NormalCitation" },
                "citationNoteNum": 0,
                "citationHash": 0
              }
            ],
            [
              { "t": "Str", "c": "bracketed citation" }
            ]
          ]
        }
      ]
    }
  ]
}"#;
        let ast = parse_document(json).unwrap();
        assert_eq!(
            get_citations_document(&ast),
            vec!["Bomhard.2024b", "BGH.2010c"]
        );
    }

    #[test]
    fn test_parse_document_invalid_format() {
        // No fallback: markdown or malformed input must be rejected
        assert!(parse_document("This is not JSON").is_err());
        assert!(parse_document("[]").is_err()); // not a Pandoc object
        assert!(parse_document(r#"{"meta": {}, "blocks": []}"#).is_err()); // missing pandoc-api-version
    }

    #[test]
    fn test_get_citation_difference() {
        let json: Vec<Citations> = vec![
            Citations {
                citation_key: ".2024".to_string(),
            },
            Citations {
                citation_key: ".2024a".to_string(),
            },
            Citations {
                citation_key: "AGGelnhausen.2024".to_string(),
            },
            Citations {
                citation_key: "Alexander.2024".to_string(),
            },
            Citations {
                citation_key: "Alexander.2024a".to_string(),
            },
        ];
        let md = vec![
            "Bomhard.2024b".to_string(),
            "BGH.2024".to_string(),
            "BGH.2010c".to_string(),
            "LGHamburg.2024".to_string(),
            "Alexander.2024".to_string(),
            "Alexander.2024a".to_string(),
        ];
        let out = vec![
            Citations {
                citation_key: ".2024".to_string(),
            },
            Citations {
                citation_key: ".2024a".to_string(),
            },
            Citations {
                citation_key: "AGGelnhausen.2024".to_string(),
            },
        ];
        // Expected output
        // inputs (steal from the prints)
        assert_eq!(get_citation_difference(&md, json), out)
    }
    #[test]
    fn test_get_citation_difference_empty() {
        let document_citations: Vec<String> = vec![];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
        ];

        let result = get_citation_difference(&document_citations, json_citations);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].citation_key, "key1");
        assert_eq!(result[1].citation_key, "key2");
    }

    #[test]
    fn test_get_citation_difference_all_match() {
        let document_citations = vec!["key1".to_string(), "key2".to_string()];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
        ];

        let result = get_citation_difference(&document_citations, json_citations);
        assert_eq!(result.len(), 0); // No differences
    }

    #[test]
    fn test_get_citation_difference_duplicates_in_document() {
        let document_citations = vec![
            "key1".to_string(),
            "key1".to_string(),
            "key3".to_string(),
        ];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
            Citations {
                citation_key: "key3".to_string(),
            },
        ];

        let result = get_citation_difference(&document_citations, json_citations);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].citation_key, "key2"); // key2 should still be the only difference
    }

    #[test]
    fn test_get_citation_difference_empty_document() {
        let document_citations: Vec<String> = vec![];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
        ];

        let result = get_citation_difference(&document_citations, json_citations);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].citation_key, "key1");
    }
    #[test]
    fn test_get_bibliography_path() {
        let mut meta = Map::new();
        meta.insert(
            "bibliography".to_string(),
            MetaValue::MetaInlines(vec![Inline::Str("a/b/c".to_string())]),
        );
        let ast = pandoc_with(meta, Vec::new());
        assert_eq!(get_bibliography_path(&ast).unwrap(), "a/b/c");
    }

    #[test]
    fn test_get_bibliography_path_string() {
        let mut meta = Map::new();
        meta.insert(
            "bibliography".to_string(),
            MetaValue::MetaString("a/b/c".to_string()),
        );
        let ast = pandoc_with(meta, Vec::new());
        assert_eq!(get_bibliography_path(&ast).unwrap(), "a/b/c");
    }

    #[test]
    fn test_get_bibliography_path_missing() {
        let ast = pandoc_with(Map::new(), Vec::new());
        assert!(get_bibliography_path(&ast).is_err());
    }

    #[test]
    fn test_format_pr_comment_uncited() {
        let report = Report {
            all_cited: false,
            uncited: vec![
                Citations {
                    citation_key: "Bomhard.2024b".to_string(),
                },
                Citations {
                    citation_key: "Alexander.2024".to_string(),
                },
            ],
            total_bibliography: 5,
            total_citations: 3,
        };
        let out = format_output(&report, &OutputFormat::Pr).unwrap();
        let expected = "<!-- zoterocoverage -->\n\
                        ## [Fail] ZoteroCoverage: 2 of 5 sources not cited\n\n\
                        The following sources in the bibliography are not cited in the document:\n\
                        - `@Bomhard.2024b`\n\
                        - `@Alexander.2024`\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn test_format_pr_comment_all_cited() {
        let report = Report {
            all_cited: true,
            uncited: vec![],
            total_bibliography: 5,
            total_citations: 5,
        };
        let out = format_output(&report, &OutputFormat::Pr).unwrap();
        let expected = "<!-- zoterocoverage -->\n\
                        ## [Pass] ZoteroCoverage: All 5 sources cited\n";
        assert_eq!(out, expected);
    }
}
