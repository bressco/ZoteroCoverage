# ZoteroCoverage

Tool to check whether all citations saved to a Zotero Library are cited in a given document. 
Expects a bibliography formatted as JSON as exported by [BetterBibTex](https://github.com/retorquere/zotero-better-bibtex/tree/master) with keys formatted matching the Regex `\w+\.\d{4}\w?` (e.g. Alexander.2024a) and a Markdown / Textdocument with equally formatted citation keys.