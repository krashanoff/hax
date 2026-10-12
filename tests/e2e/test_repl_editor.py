#!/usr/bin/env python3
"""Ctrl-G supplies recent transcript context without submitting it."""

import harness


def test_editor_context():
    home, workdir = harness.make_home()
    capture = workdir / "editor-input"
    editor = workdir / "editor.sh"
    editor.write_text(
        '#!/bin/sh\ncat "$1" > "$HAX_EDITOR_CAPTURE"\nprintf "\\n# A heading\\nnew prompt\\n" >> "$1"\n'
    )
    editor.chmod(0o700)
    term = harness.Terminal(
        "text assistant reply\nend-turn\n" * 4,
        home=home,
        workdir=workdir,
        extra_env={"EDITOR": str(editor), "VISUAL": "", "HAX_EDITOR_CAPTURE": str(capture),
                   "HAX_TRANSCRIPT": str(workdir / "transcript.txt")},
    )
    term.wait_for("❯")
    for message in ("first prompt", "second prompt", "third prompt"):
        term.type(message)
        term.send("Enter")
        term.wait_for("assistant reply")
        term.wait_for("❯")
    term.type("existing draft")
    term.send("C-g")
    term.wait_for("# A heading")
    text = capture.read_text()
    harness.expect(text.startswith("# Recent transcript (not sent):\n"), "context header")
    harness.expect("# first prompt" not in text, "only last five entries")
    harness.expect("# second prompt" in text and "# third prompt" in text, "recent prompts")
    harness.expect("# assistant reply" in text, "assistant entries")
    harness.expect(text.endswith("# --- end recent transcript; write your prompt below ---\nexisting draft"),
                   "context precedes the existing draft")
    term.send("Enter")
    term.wait_for("assistant reply")
    transcript = workdir / "transcript.txt"
    # The mock provider records the submitted prompt in the transcript log.
    harness.expect("existing draft\n# A heading\nnew prompt" in transcript.read_text(),
                   "draft and Markdown heading kept intact")
    harness.expect("Recent transcript (not sent)" not in transcript.read_text(),
                   "context never submitted")


harness.main(globals())
