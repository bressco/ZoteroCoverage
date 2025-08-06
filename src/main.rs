use clap::Parser;
use clap_file::Input;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::fs;
use std::io::Read;
use yaml_front_matter::{YamlFrontMatter, Document};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    //path to document, md format (or any plain text format)
    #[clap(short, long)]
    document: Input,

    //path to zotero_lib, JSON format; optional
    #[clap(short, long)]
    zotero_lib: Option<Input>,

}

#[derive(Deserialize, Debug, Clone, Hash, PartialEq)]
struct Citations {
    #[serde(rename = "citation-key")]
    pub citation_key: String,
}

// Get Metadata from markdown document for Library
// Bibliography field contains path
#[derive(Deserialize)]
struct Metadata {
    bibliography: String
}

impl fmt::Display for Citations {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.citation_key)
    }
}

fn get_citations_bibliography(
    bibliography: &str,
) -> Result<Vec<Citations>, Box<dyn std::error::Error>> {
    let v: Vec<Citations> = serde_json::from_str(bibliography)?;
    Ok(v)
}

fn get_citations_document(document: &str) -> Result<Vec<&str>, Box<dyn std::error::Error>> {
    let re = Regex::new(r"@(?<key>\w+\.\d{4}\w?)").unwrap();
    let md_citations: Vec<&str> = re
        .captures_iter(document)
        .map(|caps| caps.name("key").unwrap().as_str())
        .collect();

    Ok(md_citations)
}

fn get_citation_difference(
    document: Vec<&str>,
    json: Vec<Citations>,
) -> Result<Vec<Citations>, Box<dyn std::error::Error>> {
    let document_set: HashSet<&str> = HashSet::from_iter(document);

    let difference: Vec<_> = json
        .iter()
        .filter(|citation| !document_set.contains(&citation.citation_key[..]))
        .cloned()
        .collect();

    Ok(difference)
}

fn get_bibliography_path(document: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Extract the path of the bibliography given in the yaml header of the passed md file
    let metadata= YamlFrontMatter::parse::<Metadata>(&document)
        .unwrap()
        .metadata;
    Ok(metadata.bibliography)
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // TODO: Handle relative paths

    // Read in the provided md document
    let mut document_md_input = args.document.lock();
    let mut document_md: String = String::new();

    // Read the document into a string
    document_md_input.read_to_string(&mut document_md)?;

    // let mut bibliography_json_input;

    let mut bibliography_json: String = String::new();
    // Get bibliography either from CLI oder from header in document
    match args.zotero_lib {
        Some(ref zotero_lib) => {
            // If found, read in the json based on the CLI
            let bibliography_json_input = args.zotero_lib;
            bibliography_json_input.unwrap().read_to_string(&mut bibliography_json)?;
        }
        None => {
            // Get bibliography path as input

            // YAML does not accept tabs, but two or four spaces instead
            let clean_doc = &document_md.replace("\t", "  ");
            let bibliography_path = get_bibliography_path(&clean_doc).unwrap();
            // read from path
            println!("Trying to open {bibliography_path}");
            bibliography_json = fs::read_to_string(bibliography_path)?
        }
    }

    let citations_bibliography = get_citations_bibliography(&bibliography_json).unwrap();
    let citations_document = get_citations_document(&document_md).unwrap();

    let differences = get_citation_difference(citations_document, citations_bibliography).unwrap();

    if differences.len() == 0 {
        println!("All sources cited");
    } else {
        println!("{} Sources not cited:", differences.len());
        for d in differences {
            println!("{d}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        Citations, get_citation_difference, get_citations_bibliography, get_citations_document,
    };

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
        let testdata_md = r#"Gemeinsame Voraussetzung beider Schranken ist zunächst, dass der
Zugang zu den Daten rechtmäßig erfolgt.[@Bomhard.2024b Rn. 15] Dieser
kann etwa auf einer dahingegenden Lizenz beruhen (welche aber etwa
kein TDM zulässt) oder auch auf einer Einwilligung der:des
Berechtigten. Eine solche kann sich (konkludent) durch öffentliche
Zugänglichmachung im Internet ergeben.[@BGH.2024 Rn. 45--47;
@BGH.2010c Rn. 36; so auch @LGHamburg.2024 Rn. 86] Das wird meist der
Fall sein, zumindest, was den Zugang zu den Daten betrifft. @Alexander.2024; @Alexander.2024a
"#;
        assert_eq!(
            get_citations_document(testdata_md).unwrap(),
            vec![
                "Bomhard.2024b",
                "BGH.2024",
                "BGH.2010c",
                "LGHamburg.2024",
                "Alexander.2024",
                "Alexander.2024a"
            ]
        );
    }

    #[test]
    fn test_get_citations_document_invalid_format() {
        let testdata_md = "Here is a citation @key.1991 and another @key.2002. Invalid @key.";
        let result = get_citations_document(testdata_md).unwrap();
        assert_eq!(result.len(), 2); // Should still extract key1 and key2
        assert_eq!(result[0], "key.1991");
        assert_eq!(result[1], "key.2002");
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
            "Bomhard.2024b",
            "BGH.2024",
            "BGH.2010c",
            "LGHamburg.2024",
            "Alexander.2024",
            "Alexander.2024a",
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
        assert_eq!(get_citation_difference(md, json).unwrap(), out)
    }
    #[test]
    fn test_get_citation_difference_empty() {
        let document_citations: Vec<&str> = vec![];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
        ];

        let result = get_citation_difference(document_citations, json_citations).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].citation_key, "key1");
        assert_eq!(result[1].citation_key, "key2");
    }

    #[test]
    fn test_get_citation_difference_all_match() {
        let document_citations = vec!["key1", "key2"];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
        ];

        let result = get_citation_difference(document_citations, json_citations).unwrap();
        assert_eq!(result.len(), 0); // No differences
    }

    #[test]
    fn test_get_citation_difference_duplicates_in_document() {
        let document_citations = vec!["key1", "key1", "key3"];
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

        let result = get_citation_difference(document_citations, json_citations).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].citation_key, "key2"); // key2 should still be the only difference
    }

    #[test]
    fn test_get_citation_difference_empty_document() {
        let document_citations: Vec<&str> = vec![];
        let json_citations = vec![
            Citations {
                citation_key: "key1".to_string(),
            },
            Citations {
                citation_key: "key2".to_string(),
            },
        ];

        let result = get_citation_difference(document_citations, json_citations).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].citation_key, "key1");
    }
}
