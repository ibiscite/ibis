# Use a downloaded Overleaf bibliography

In your Overleaf project, choose **File > Download > Download as source
(.zip)**. See [Overleaf's download instructions](https://docs.overleaf.com/managing-projects-and-files/downloading-a-project).
Use your operating system's archive tool to extract the download, then find
each `.bib` file you want to use. Ibis reads these local files without signing
in or making requests to Overleaf.

In a terminal, change to the extracted directory. For example, if its two
bibliographies are `references.bib` and `supplement.bib`, select both explicitly:

```sh
ibis search lantern --file references.bib --file supplement.bib
ibis search lantern --file references.bib --file supplement.bib --json
ibis cite vale2024 fen2023 --format cite --file references.bib --file supplement.bib
ibis cite vale2024 fen2023 --format cite --file references.bib --file supplement.bib --json
```

Replace the search word, keys and file names with your own. The citation in
this example is `\cite{vale2024,fen2023}`; paste it into your document.
Only the selected files are searched. A writing-bibliography check names one
file and checks that file alone.

When the project changes, download and extract it again, replace the local
bibliography files with the completed download, and run the commands again to
read the new contents.
