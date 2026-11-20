# ZoteroCoverage

Tool to check whether all citations saved to a Zotero Library are cited in a given document. 
Expects a bibliography formatted as JSON as exported by [BetterBibTex](https://github.com/retorquere/zotero-better-bibtex/tree/master) 
using the Better CSL filter and a JSON-formatted document.

The JSON-formatted document should be created using pandoc, e.g. by calling `pandoc -t json document.md -o output.json`.