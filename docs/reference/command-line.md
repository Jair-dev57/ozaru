# Command line

ozaru has a command with the same functions, for the terminal and for scripts. With
[uv](https://docs.astral.sh/uv/) nothing has to be installed:

```bash
uvx ozaru text report.pdf
```

If ozaru is installed, `ozaru text report.pdf` and `python -m ozaru text report.pdf` do the same.

`FILE` is the path of a PDF, the `http://` or `https://` URL of one, or `-` to read it from
the standard input:

```bash
cat report.pdf | uvx ozaru count -
uvx ozaru info https://example.com/report.pdf
```

Every command takes `-p PASSWORD` for protected PDFs.

| Command | What it does |
|---|---|
| `text FILE [--pages 1,3-5]` | prints the text, with a form feed between pages |
| `count FILE` | prints the number of pages |
| `info FILE` | prints, as JSON, the metadata, the permissions and the size of each page |
| `words FILE [--json]` | every word with its page, block, line and box, separated by tabs |
| `blocks FILE [--json]` | every block of text with its box |
| `annotations FILE [--json]` | the annotations |
| `fields FILE [--json]` | the fields of the form with their values |
| `search FILE TEXT [--case-sensitive] [--pages 1,3] [--json]` | where the text is: page, box and text, separated by tabs; exit 1 if it isn't there |
| `spans FILE [--json]` | the pieces of text with one font, size and color |
| `tables FILE [--strategy lines\|text\|both] [--json]` | the tables, as Markdown |
| `markdown FILE [--page-markers] [--strategy lines\|text\|both]` | the document as Markdown |
| `bookmarks FILE [--json]` | the bookmarks, indented by level, with their pages |
| `labels FILE` | the label of each page (`i`, `ii`, `1`, `A-1`...) |
| `attachments FILE [-o FOLDER]` | lists the attached files, or saves them to a folder |
| `images FILE [-o FOLDER]` | saves the images to a folder (`images` by default) |
| `render FILE PAGE -o OUT.png [--scale 2] [--no-annotations]` | draws a page; `-o -` writes to the standard output |
| `rotate FILE DEGREES -o OUT.pdf [--pages 1,2]` | turns pages clockwise |
| `extract-pages FILE PAGES -o OUT.pdf` | saves those pages, in that order, such as `3,1,5-7` |
| `split FILE -o FOLDER [--pages-per-file N]` | saves each page, or group of pages, as a file |
| `merge FILE FILE... -o OUT.pdf` | joins PDFs |
| `set-metadata FILE -o OUT.pdf [--title T] [--author A]... [--remove NAME]` | changes the information: `--title`, `--author`, `--subject`, `--keywords`, `--creator`, `--producer` |
| `delete-pages FILE PAGES -o OUT.pdf` | removes pages, such as `2,5-7` |
| `crop FILE X0 Y0 X1 Y1 -o OUT.pdf [--pages 1,2]` | crops to a box measured from the top left |
| `blank-page FILE -o OUT.pdf [--position N] [--width W] [--height H]` | adds an empty page |
| `attach FILE ATTACHMENT -o OUT.pdf [--name NAME] [--description TEXT]` | attaches a file |
| `watermark FILE TEXT -o OUT.pdf [--pages 1,2] [--color red] [--opacity 0.3] [--size 60] [--rotation 45] [--font NAME]` | writes a text across the middle of the pages |
| `highlight FILE TEXT -o OUT.pdf [--kind highlight\|underline\|strikeout\|squiggly] [--color red] [--note TEXT] [--pages 1,2] [--case-sensitive]` | marks every place a text is found |
| `add-note FILE PAGE X Y TEXT -o OUT.pdf [--author NAME] [--open]` | puts a sticky note on a page |
| `add-link FILE PAGE X0 Y0 X1 Y1 -o OUT.pdf (--uri URL \| --to-page N) [--border 1]` | makes an area of a page a link |
| `delete-annotations FILE -o OUT.pdf [--pages 1,2] [--subtype Highlight] [--author NAME] [--contains TEXT]` | removes the annotations that match (all, if none given) |
| `add-image FILE PAGE X Y IMAGE -o OUT.pdf [--width 80] [--height 60] [--opacity 1] [--rotation 0]` | puts a JPEG or PNG picture on a page, its top left corner at a position |
| `add-text FILE PAGE X Y TEXT -o OUT.pdf [--size 12] [--color black] [--font NAME] [--rotation 0]` | writes a text at a position measured from the top left of the page |
| `encrypt FILE -o OUT.pdf --user-password SECRET [--owner-password OTHER]` | protects it with AES-256 |
| `fill FILE name=value... -o OUT.pdf` | fills fields; `true` and `false` for check boxes |

A problem is a line starting with `ozaru:` on the standard error and the exit code 1; wrong
arguments exit with 2. `ozaru text file.pdf | head` ends quietly when `head` does.
