---
name: setup
description: Set up a fresh clone of magym, including the Magic: The Gathering Comprehensive Rules in docs/rules/. Use when the user asks to set up the project, when docs/rules/ is missing, or when the user wants to update the rules to a new version.
---

# Setup

Run every command from the repository root.

The Comprehensive Rules are copyrighted and can't be committed, so the user gives
a link to them, you download them, split them into small files under
`docs/rules/` (gitignored), and delete the download. Don't write the link into
any tracked file.

## Steps

1. If `docs/rules/README.md` already exists, tell the user which version is
   installed (its "These rules are effective as of …" line) and ask whether to
   replace it. Stop if they don't.
2. Ask the user for a URL to the Comprehensive Rules as a plain-text (`.txt`)
   file. Wait for it.
3. Download it, replacing any spaces in the URL with `%20`:
   `curl -fsSL -o docs/rules.txt '<url>'`. If curl fails, show the user its
   error and go back to step 2.
4. Check the file without reading it all (it's about 1 MB). If any check fails,
   tell the user what's wrong, delete `docs/rules.txt`, and go back to step 2.
   - `grep -m1 -v '^[[:space:]]*$' docs/rules.txt` prints
     `Magic: The Gathering Comprehensive Rules`. Otherwise it's not the rules
     (e.g. an HTML page or a PDF).
   - `grep -c -E '^(Glossary|Credits)[[:space:]]*$' docs/rules.txt` prints at
     least 4 (each is in the table of contents and is a heading). Otherwise the
     file is incomplete.
   - `grep -c -E 'â€|Â' docs/rules.txt` prints 0. Otherwise the text encoding is
     garbled.
5. Split it: `rm -rf docs/rules`, write the awk program below to a file in your
   scratchpad, and run
   `LC_ALL=C gawk -v out=docs/rules -f <file> docs/rules.txt docs/rules.txt`
   (the input is passed twice on purpose: two passes). It needs GNU awk; if
   `gawk` is missing, ask the user to install it.
6. Delete the download: `rm docs/rules.txt`.
7. Report to the user the effective date (the third line of
   `docs/rules/README.md`) and the number of files
   (`find docs/rules -name '*.txt' | wc -l`, about 440).

## Layout

`docs/rules/` must look like this, since `AGENTS.md` and `README.md` there rely on it:

- `00-introduction.txt`: everything before "Contents", then the credits.
- `<n>-<chapter-slug>/<NNN>-<rule-slug>.txt`: one file per rule `NNN`, with all
  its subrules, e.g. `7-additional-rules/704-state-based-actions.txt`.
- Rules 701 and 702 are directories instead, with one file per subrule, e.g.
  `7-additional-rules/702-keyword-abilities/702.19-trample.txt` (and
  `702.1-general.txt`).
- `glossary/<letter>.txt`: glossary terms by first letter.
- `README.md`: the effective date, how to find a rule, and an index of every file.

The table of contents also contains the chapter headings, "Glossary" and
"Credits": the body starts at the second "1. Game Concepts", and the real
"Glossary" and "Credits" are the last ones. Blank lines in the source contain a
no-break space. Glossary terms aren't strictly alphabetical, so a letter's file
can be appended to more than once.

## Split program

```awk
function slug(s) {
  s = tolower(s); gsub(/\342\200\231|'/, "", s); gsub(/[^a-z0-9]+/, "-", s)
  gsub(/^-|-$/, "", s); return s
}
# Appends the buffered lines to the current file, without leading/trailing blanks.
function flush(  i, j) {
  if (path != "") {
    for (i = 1; i <= n && buf[i] == ""; i++);
    for (j = n; j >= i && buf[j] == ""; j--);
    if (i <= j && path in seen) print "" >> path
    for (; i <= j; i++) print buf[i] >> path
    seen[path] = 1
    close(path)
  }
  path = ""; n = 0
}
function start(p) {
  flush(); path = out "/" p
  system("mkdir -p '" substr(path, 1, match(path, /\/[^\/]*$/) - 1) "'")
}
{ sub(/\r$/, ""); sub(/([ \t]|\302\240)+$/, "") }
NR == FNR {
  if ($0 == "Contents" && !contents) contents = FNR
  if ($0 == "Glossary") glossary = FNR
  if ($0 == "Credits") credits = FNR
  if (/^1\. / && ++ones == 2) body = FNR
  if (/^These rules are effective/ && !effective) effective = $0
  L[FNR] = $0; total = FNR; next
}
FNR == 1 {
  start("00-introduction.txt")
  for (i = 1; i < contents; i++) buf[++n] = L[i]
  buf[++n] = ""
  for (i = credits; i <= total; i++) buf[++n] = L[i]
  flush()
}
FNR >= body && FNR < glossary {
  if (match($0, /^([1-9])\. (.+)$/, m)) {
    flush(); chap = m[1] "-" slug(m[2]); idx[++ni] = "\n### " $0 "\n"; next
  }
  if (match($0, /^([0-9][0-9][0-9])\. (.+)$/, m)) {
    if (m[1] == "701" || m[1] == "702") {
      dir = chap "/" m[1] "-" slug(m[2]); start(dir "/" m[1] ".1-general.txt")
      idx[++ni] = "- " $0 " — `" dir "/`"; subs = ++ni
    } else {
      start(chap "/" m[1] "-" slug(m[2]) ".txt")
      idx[++ni] = "- " $0 " — `" chap "/" m[1] "-" slug(m[2]) ".txt`"
    }
  } else if (match($0, /^(70[12])\.([0-9]+)\. (.+)$/, m) && m[2] != "1") {
    start(dir "/" m[1] "." m[2] "-" slug(m[3]) ".txt")
    idx[subs] = idx[subs] (idx[subs] == "" ? "  - " : ", ") m[1] "." m[2] " " m[3]
  }
  if (path != "") buf[++n] = $0
}
FNR == glossary { flush() }
FNR > glossary && FNR < credits {
  # A term is a non-blank line after a blank line and before a non-blank one.
  if ($0 != "" && L[FNR-1] == "" && L[FNR+1] != "") {
    c = tolower(substr($0, 1, 1)); key = c ~ /[a-z]/ ? c : "0-9"
    if (key != gkey) { start("glossary/" key ".txt"); gkey = key }
  }
  if (path != "") buf[++n] = $0
}
FNR == credits { flush() }
END {
  r = out "/README.md"
  print "# Game rules\n\n" effective "\n" > r
  print "Generated by the `setup` skill — do not edit by hand.\n" > r
  print "## How to find a rule\n" > r
  print "- Rule `NNN` (and all its subrules `NNN.x`) is in `<chapter>/NNN-<title>.txt`." > r
  print "- Exceptions: rules 701 (keyword actions) and 702 (keyword abilities) are directories" > r
  print "  with one file per keyword, e.g. `7-additional-rules/702-keyword-abilities/702.19-trample.txt`." > r
  print "- Glossary terms are in `glossary/<first letter>.txt`." > r
  print "- To locate a rule by number or term: `grep -rn '^704.5k' docs/rules` or" > r
  print "  `grep -rln -i 'deathtouch' docs/rules`. Read only the files you need.\n" > r
  printf "## Index\n" > r
  for (i = 1; i <= ni; i++) print idx[i] > r
  printf "\n### Glossary\n\n" > r
  PROCINFO["sorted_in"] = "@ind_str_asc"
  g = ""; for (k in seen) if (k ~ /\/glossary\//) { sub(/.*\//, "", k); g = g (g == "" ? "" : ", ") "`glossary/" k "`" }
  print g > r
}
```
