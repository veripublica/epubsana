# epubsana

epubsana repairs EPUB files. It takes what [epubveri](https://github.com/veripublica/epubveri) finds wrong with a book and fixes the problems that have one right answer. It's for anyone holding a book that doesn't pass validation, whether you read ebooks, make them, or build software around them.

You see every fix before it happens and say yes or no to each one. Then it checks the book again, undoes any fix that made things worse, and tells you exactly what changed. Your original stays as it was, and whatever no fix touched is copied across byte for byte.

It won't make every book valid, and it isn't meant to. If a repair would mean guessing what the author wanted, it leaves that alone and says so, because that part is an editor's job.

The easiest way to use it is [the web page](https://veripublica.github.io/epubsana/). Your book stays in your browser and nothing is uploaded.

For the command line, download a binary from the [latest release](https://github.com/veripublica/epubsana/releases/latest), or install it with `cargo install --locked epubsana`. Then run:

```sh
epubsana -i book.epub
```

This asks about each fix and writes `book_fixed.epub` next to your book. Add `--dry-run` to see the list without changing anything.

I test it on my own library of real books, but that's one person's shelf. If a repair damages a book, please [open an issue](https://github.com/veripublica/epubsana/issues). That's the report I most want.

It's also a Rust crate, and an npm package called `@veripublica/epubsana-wasm`. The options, exit codes, the API, limits and how to build it are in [AGENTS.md](https://github.com/veripublica/epubsana/blob/main/AGENTS.md), and what each fix does and when it declines is in [docs/FIXERS.md](https://github.com/veripublica/epubsana/blob/main/docs/FIXERS.md).

The books you repair are yours, and you can use the tool commercially for free. The code is AGPL-3.0-only, or under a commercial licence if you want to embed it in a closed-source product or run a modified version as a service. [LICENSING-FAQ.md](https://github.com/veripublica/epubsana/blob/main/LICENSING-FAQ.md) has the details. I can't accept pull requests yet, and [CONTRIBUTING.md](https://github.com/veripublica/epubsana/blob/main/CONTRIBUTING.md) explains why.
