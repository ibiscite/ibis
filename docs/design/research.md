# User research

Ibis gives people one shortcut to cite from their library in any app. Watch
people use the picker, Settings, the command line and AI-app (MCP) tools to
learn where that workflow holds up and where it breaks. Start in the public
beta, use the findings to order 0.4 work, and revisit the questions on builds
that implement the decisions. The beta study has a
[working issue](https://github.com/ibiscite/ibis/issues/2021). Release
qualification follows the [roadmap](../../ROADMAP.md#definition-of-done-for-10).

## Questions before 1.0

| What must we learn? | What it informs |
| --- | --- |
| Can people tell similar works apart at a glance when a title is long, even when moving quickly by keyboard or using Larger Text? What do they check before sending the citation? | The launcher row recipe, fixed slots and how closed parts show detail. Compare with the Spotlight and System Settings recipes in `docs/design/macos.md`. [0062](https://github.com/ibiscite/ibis/blob/8d60e9bbe5efd15b48988fbeb04e4b0a8dd5d60c/docs/decisions/0062-closed-parts-owned-space-fitted-layers.md) |
| Can people read and reach drawer choices and a Settings search result in a small window? Where do their eyes or focus go when content moves, including in German and Arabic? | The shared menu boundary, fallback view and scroll reveal rules, and which visible states the canvas gate should cover. 0062 |
| How do people type a locator or separate works without instruction? When do numbers or punctuation mean part of a title, and when do people reach for the drawer? | The shared citation grammar, localised locator terms, first-use teaching and the drawer's role. [0063](https://github.com/ibiscite/ibis/blob/05f7f0a211da972dba9d977930136e0a379a1ba7/docs/decisions/0063-form-follows-function.md) |
| When output is wrong for Word, Google Docs or LaTeX, where do people look to fix it? Can they predict where a format choice will apply next time? | Formats only in the picker, how it names this paste, app, site or project, and which choice it teaches. 0063 |
| On reopening with an empty search, what work do people expect to see? Can they find one they just cited and tell it apart from earlier recents? | Session history followed by recents in empty search and the placement of list commands. 0063 |
| With a library missing or paste unavailable, can people find the one status line, explain what happened and take the offered next step? | The heading line's wording, its one fix and spoken feedback. 0063 |
| When people move a draft between the picker and a command, AI app or editor, what do they expect to carry over? Can they finish it with the same works, order and locators, and explain the format and destination? | The shared draft and which context travels with it, the shared status and recovery, and what to settle before the public contract freezes. [0064](https://github.com/ibiscite/ibis/blob/05f7f0a211da972dba9d977930136e0a379a1ba7/docs/decisions/0064-one-citation-one-status-generated-surfaces.md) |

On available builds, test use before explaining these ideas. For planned
behaviour, show the same invented example in a prototype, record reactions
as concept evidence and run the task again after implementation. Record the
build and conditions with each observation.

## Who to invite

| Workflow | Recruit people who already do this |
| --- | --- |
| Students with Zotero | Save readings to Zotero and cite them while writing assignments. |
| Researchers with BibTeX/LaTeX | Edit a `.bib`, rely on citation keys and write in LaTeX, including Overleaf. Include people who already use the command line in this workflow. |
| Writers in Word or Google Docs | Insert and revise citations in a word processor. Include people who expect Zotero-managed citations and people who use formatted text. |
| People with a folder of PDFs | Find sources by file name or PDF contents and assemble references by hand. |
| Keyboard-only users | Use shortcuts and focus order to write and switch apps without a pointer. |
| Screen-reader users | Read results, act on controls and check feedback using their usual screen reader. |
| German interface users | Search, read controls and enter locators while working in German. |
| Arabic interface users | Search and enter locators in Arabic, including mixed Arabic and Latin titles or keys. |
| AI-app users | Ask a tool-enabled AI app for a source or citation and check it before using it in their writing. |

Recruit through the beta, a GitHub Discussion and existing Zotero, TeX and
accessibility communities. Ask how people keep, find and cite references and
which apps, input methods, assistive tools and interface language they use.
Aim for 5 distinct people per segment across rounds. Count someone in each
segment whose workflow they actually exercise. Start with 6–8 people across
the main writing workflows, include people new to Ibis, and fill coverage
gaps in later rounds. Track who is still missing by segment and workflow.

## Methods

Time and cost are estimates for one maintainer. Reuse task cards, libraries
and scoring across rounds.

| Method | What it answers | Sample | Time | Cost |
| --- | --- | --- | --- | --- |
| Moderated usability tests | Where people get stuck, what they expect, and whether they can verify and recover from the result | 5 per segment across rounds, with overlap | 40–45 min per person and 15 min to tidy notes | Maintainer time, no paid tooling |
| Unmoderated beta tasks | Which steps people can complete on their own build and setup, and what errors recur | 8–15 volunteers, with tasks for their workflow | 15–20 min per tester and 1–2 hours to review a batch | Volunteer and maintainer time |
| Short survey outside the app | What people last tried, how often the job comes up and which workflows to recruit next | Opt-in beta and community replies | 3 min to reply and 1 hour to review | One voluntary text form or private reply, no paid tooling |
| Diary study | How search, history, remembered formats and hand-offs hold up across writing days | 3–5 people in a multi-week writing routine | 2–3 weeks, 2–3 min per entry and a 15 min wrap-up | Participant time and about 2 hours to review |
| GitHub issue and discussion review | Recurring problems, workarounds and gaps to test | Relevant threads since the last round | 1 hour per round | Maintainer time |

Send the same task cards to beta testers. Ask them to report what happened,
the build, approximate time and difficulty by hand. In the survey, ask what
they last tried to cite with, which app they wrote in, where they got stuck,
what they did next and whether they would volunteer for a session. For the
diary, ask for up to two short entries a week from a parallel invented draft
in their usual writing tools. Ask what they wanted to do, what they did, what
happened and what they had to redo. Accept shorter entries and skipped days.
Ibis has no telemetry. The study uses voluntary replies and session notes.
[Privacy and access](#privacy-and-access)

## Reference journeys

Use the named [canvas scenarios](../../apps/desktop/canvas/scenarios.ts) for
the starting content. Reuse their invented records and labels in the study
library and use the same expected results in canvas follow-up checks. The
session adds the native destination and the actions between boards. Check the
build, destination, format and expected output before the session.

### 1. One long title into Word

- The `long-titles` board gives the long title and six authors. Use
  one copy of that record in the study library and add a near match with a
  different year, so there is a visible way to tell them apart.
- **Ask** the participant to cite the work on the task card at the marked
  cursor in the invented Word draft. The card gives its title and year.
- **Check** that the intended work appears once at the cursor as an APA
  in-text citation. Score Copy followed by a manual paste if the build uses
  that delivery path. Record which path the person used.
- **Observe** search words, signs used to distinguish the works, any need
  to see the full title or authors, and what people expect Enter to do.

### 2. Several works with a chapter locator

- Start from `citation-details-larger` for the drawer with Larger Text,
  `shelf` for several works, and `show-as-menu` for an open drawer menu. Use
  the two invented references from `shelf` and run in the Small picker.
- **Ask** the participant to cite both works together in the scratch draft,
  pointing to Chapter 3 of the second work. Use a format that can express
  the locator.
- **Check** that the output has both works and Chapter 3 belongs only to the
  second. Check that drawer choices can be reached and read, and closing
  a menu restores useful focus.
- **Observe** how people add the second work, which token they edit, and
  whether menus clip, scroll, or keep the expected focus. After the timed
  attempt, open each drawer menu in turn to check the state even when the
  person chose to type the locator.

### 3. Change library, cancel, find a setting

- Start from `library-menu` for the invented library selector and
  `settings-look` for the Text size row. For the slow-load check, run the
  same Settings search with content above the target delayed on a test build.
- **Ask** the participant to start choosing a different library and cancel
  before accepting it. Have them use Settings search to find the control
  that makes text easier to read.
- **Check** that the person returns to their search in the original library
  and that the Text size control is visible, reachable and focused after
  the Settings page settles.
- **Observe** how people start and back out, what they think was saved,
  which search words they try, and where visual and spoken focus land as
  the page scrolls and content arrives.

## Interview guide

Ask about a recent instance. Have the person describe it from memory, or
recreate it with the invented materials.

| Stage | Prompts |
| --- | --- |
| Warm-up | What kinds of things have you been writing lately? When did you last add a reference? |
| Context | Where was the source? What were you writing in? How did you move between the two? |
| Deep dive | Talk me through that citation from deciding what to cite to checking it in the draft. What changed when you needed a page, a chapter or another work? What happened the last time something went wrong? |
| Concept reaction | With an invented example, what do you think the picker would do with `vale 2024 p12; aster ch3`? How would you check? Where would you look to change this output? What would you expect to see on reopening? What would carry over if you continued in another tool? |
| Wrap-up | What would you keep from your usual workflow? What step would you go back and change? What did we miss? |

Show one concept at a time after the tasks it could teach. Ask what people
expect before showing the result. Follow up with “What made you think that?”
or “Can you walk me through what you just did?”

## Moderated test script

Before the call, prepare a fresh Ibis profile, the shipped sample library,
an invented study library, a second invented library, and scratch drafts.
Set the study's Word format to APA after first run.
Reset to a known starting state for each participant. Use their usual input
and access setup. Start with consent and a few context questions, allow about
25 minutes for tasks, and leave the last few minutes for concept reactions
and wrap-up.

Say, “We’re testing Ibis. Use it as you normally would. Tell me what you’re
looking for when that feels comfortable. You can pause or skip anything.”
Read each goal as written and let people choose their own route. If they are
stuck or ask for help, first ask what they would try next. After recording the
block, give the smallest hint needed or move to the next starting state.

| Task | Read to the participant | Check |
| --- | --- | --- |
| 1. First run | You’ve just opened Ibis. Try it with a reference about lantern gardens using the material provided. | Gets to a usable sample library and completes the practice citation. Note any hesitation around permissions. |
| 2. Summon | You are writing in this Word draft. Open Ibis from here. | Picker opens with Word as the destination. Note whether the person recalls or finds the shortcut. |
| 3. Find | Find the work on this card for the marked sentence. Show me when you think you have it. | Selects the intended long-title result from Journey 1. |
| 4. Deliver | Put that citation into the marked sentence. | Correct work and format appear exactly once at the marked cursor. |
| 5. Multi-cite and locator | Put both works on this card in one citation in the scratch draft. For the second, point to Chapter 3. | Both works and the locator are correct, as in Journey 2. |
| 6. Recover | Take the second work out of this prepared citation. Now you need it back just as it was. | Finds Undo and restores the token in its place with its locator. |
| 7. Cancel a library change | Start choosing the other practice library. Stop when you get to the choice and return to what you were doing. | Cancels the change and returns to the original library and search. |
| 8. Settings | Use Settings search to find where you could make the text easier to read. Leave it as it is. | Reaches Text size and can tell which value is selected. |

After task 1, load the study library. Before task 5, set Small and Larger
Text. For task 6, reopen a prepared two-work draft with the same locator.
Record the result of each task before resetting. Use comparable invented
sources and scratch targets for each workflow. Report the actual source,
destination and access setup.

Use focused rounds for the other picker questions. Ask someone to cite again
from a blank search after two deliveries. Ask them to choose a format for one
draft, then predict what a different app or site will get. With an invented
library unavailable, ask them to resume citing and say what they think
happened. Record the first place they look and any recovery they try.

With people who use more than one surface, follow up on a build with shared
drafts. Ask them to start a citation with two works and a locator in a tool
they use, continue it in the picker and finish in the scratch draft. Repeat
in the other direction where possible. Check what carries over, what the
destination does to the format and whether a refusal offers the same way
forward. Use only invented data in the AI client.

Use the same measures for each task.

- **Success** is completing the stated check unaided or with help. Record
  failures, skips and unavailable paths as such.
- **Time** runs from the end of the prompt to a correct result or stop, with
  breaks paused. Report the median and range for unaided successes and the
  number of attempts. Note time from first launch to first correct citation
  on an uninterrupted first-run path.
- **Errors** include wrong works, locators, formats or destinations, missed
  controls and failed recovery attempts. Count them and note whether the
  person recovered unaided. Note any hints given.
- **Ease (SEQ)** is the reply to “Overall, how difficult or easy was this
  task?” on a 1–7 scale (1 very difficult, 7 very easy). Offer it in the
  participant’s preferred accessible format immediately after the task.

Rate each observed barrier by its effect on the task and the recovery it
allows. A single instance can show a serious barrier. Report counts with
denominators and tested conditions.

| Severity | Effect |
| --- | --- |
| Critical | A wrong citation can pass as correct, or the core task is blocked with no workable recovery. |
| Major | The person needs help, a restart or a costly workaround to finish. |
| Minor | The person finishes unaided after avoidable confusion or effort. |
| Low | A small point of friction with no observed effect on completion. |

## Turn observations into changes

1. Put one de-identified observation per card on a private affinity map.
   Include task, build, segment and conditions. Group cards by the job and
   barrier, then look for counterexamples and differences across workflows.
2. Write each finding as the observed behaviour, its effect and the evidence
   behind it. Record severity, the number of relevant attempts and any open
   question. Label evidence by method.
3. File one GitHub issue per finding and link it from the
   [tracking issue](https://github.com/ibiscite/ibis/issues/2014). Include the
   journey, invented reproduction, and the decision or design rule it bears on.
4. Name the owning doc and the next check. Use [picker](picker.md),
   [delivery](delivery.md), [source checks and sheets](sources-and-sheets.md),
   [Settings](settings.md) or the macOS recipe page for the rule. When the
   maintainer makes or revises a decision, update its record and the owning
   page that day and link the change from the tracking issue. Match the
   repo's [design process](../design.md#how-the-design-changes).
5. Replay the named canvas case and retest the changed task with people in
   the affected workflow. Put the outcome on the finding issue.

## Privacy and access

- Use invented libraries, PDFs and drafts for all study tasks. Use clean
  practice profiles and share only the test window if screen sharing helps.
  Ibis ships [sample references](choice-screens.md) for first run.
- Before collecting any notes, explain the purpose, what will be kept and
  shared, that taking part is voluntary, and when the person can withdraw.
  Ask for consent. Let people skip questions or stop at any point.
- Keep de-identified task notes as the only session record. Never record real
  library data, private document text, paths, account names, credentials or
  private prompts. If private material appears, pause sharing. Ask beta
  testers and diary participants for invented examples.
- Keep contact and consent details apart from the notes, accessible only to
  the maintainer. Explain how to withdraw notes before aggregation. Delete
  contact details, consent details and working notes within 30 days of
  synthesis. Publish de-identified findings and invented reproductions.
  Omit combinations of details that could identify someone in a small group.
- Offer task cards in accessible plain text in advance, with the option to
  use a familiar keyboard, screen reader, text size and language. Offer
  spoken or typed replies, captions, breaks and more time. Sharing a screen
  and thinking aloud are optional. Check what works for the person before
  starting and record the access conditions with the task result.
- All feedback is voluntary and outside Ibis. Keep the app's no-telemetry and
  data rules in every round, including when an AI client is used.
  [Privacy and data flows](../../PRIVACY.md#network-and-diagnostics)
