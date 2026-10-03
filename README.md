# PastePilot

[PastePilot_0.1.0_aarch64.dmg](https://github.com/MichelMitri1/pastePilot/releases/download/v1.0.0/PastePilot_0.1.0_aarch64.dmg)

A macOS menu bar app that drafts support replies in your voice.

**Select a student's message → press ⌥R → the reply appears in the chat input.**

It never presses Enter. You always review and send the reply yourself.

## What it does

1. **Captures the selection.** It uses the Accessibility API first. If that fails, it sends ⌘C and then restores your clipboard exactly.
2. **Remembers the conversation.** The message is stored locally in the conversation for that ticket or chat page. The last few turns go along as context, so "I sent it above" makes sense.
3. **Detects the mode.** It sorts the request as Technical, Mentoring, Billing, Career or General with an instant local classifier. You can override it.
4. **Retrieves relevant knowledge and examples.** It pulls only the knowledge-base entries and past replies that match this message, using SQLite full-text search in under a millisecond.
5. **Streams the reply** from OpenAI and puts it on the clipboard. If the request fails, the clipboard is left untouched.
6. **Pastes the reply** into the text input you were using, or the one you clicked while it was generating. If there's no input, it shows "Reply copied to clipboard."
7. **Shows the rewrite bar.** You can pick Shorter, Friendlier, Professional, Explain more or Regenerate, change the mode, or save the reply as an example. The new version replaces the old one in place.

A small status pill shows progress and the detected mode, for example "Generating · Technical".

## Features

### Conversation memory
- **Scoping.** A conversation is keyed by the app and the page URL, which carries the ticket and channel IDs. Pages without a URL use the window title instead, ignoring unread counters like "(3)". Two tickets or chats never share context.
- **Your real reply is remembered.** After a reply is pasted, PastePilot watches that text box. When you send and the box empties, the last text in it, including your edits, replaces the generated draft in memory. If you switch tickets without sending, nothing is saved. No keystrokes are recorded, and the box is only checked while a draft is waiting, for up to 20 minutes without changes.
- **Size limit.** Only the most recent turns that fit the limit are sent. The defaults are 6 messages and 3,000 characters, both configurable.
- **Expiry.** A conversation that's idle longer than the expiry, 12 hours by default, starts fresh.
- **Re-selecting the same message** regenerates the reply without duplicating anything in memory.
- **Manual control.** Use **New Conversation** from the menu bar or ⌥⇧R, **Clear Current Conversation**, or turn off auto-detect for one manual session.
- **Off switch.** You can turn memory off entirely in the menu bar or in Settings.

### Learning your reply style
- **The library.** You can add, edit, delete and bulk-import reply examples. Each has the student message, your reply and an optional category.
- **What gets sent.** Each reply includes only the top few matching examples, 3 by default. A category matching the detected mode ranks higher.
- **A measured style profile.** It covers typical length, :) rate, exclamation marks, questions and paragraphing. It's calculated from all your saved replies and cached in the system prompt.
- **★ Save on the rewrite bar** saves the current exchange. If you edited the reply in the chat box, your edited version is what gets saved.

### Smart modes
- **Local classification** by keyword and code-pattern scoring. It takes microseconds and makes no AI call.
- **Follow-ups keep the mode.** A short follow-up like "I sent it above" inherits the conversation's previous mode.
- **Instructions per mode** are editable under **AI** in Settings.
- **Overrides.** You can set a sticky override from the menu bar **Mode** submenu or in Settings. You can also override a single reply with the mode picker on the rewrite bar, which regenerates it.

### Knowledge base
- **Entries** have a title, content, category, tags and an enabled flag. Manage them in Settings, or bulk import JSON or Markdown.
- **Only matching entries are sent,** 3 at most and within a size limit. Billing and career messages also search for related words, so "money back" finds "Refund policy".
- **No invented facts.** The model is told never to state policies, prices, dates or links that aren't in the knowledge base or the student's message.
- **Billing is stricter.** For billing messages the model may rely only on the knowledge base. With no matching entry, it must ask a question or say it will confirm.

### Quick rewrites
- **Five actions:** Shorter ⌥1, Friendlier ⌥2, More professional ⌥3, Explain more ⌥4 and Regenerate ⌥5. They work from the rewrite bar, the keys, or the menu bar **Rewrite Last Reply** submenu.
- **Fast.** They reuse the original request's context, with no new search, plus the current draft.
- **Replaced in place.** The old reply is found in the text box, selected, checked, and pasted over. If you already edited it, the new version only goes to the clipboard.
- **Temporary keys.** ⌥1–⌥5 are only taken while the bar is visible, which is 30 seconds after a reply.

### GitHub Debug Mode
Use it for small bugs in a student's **public** GitHub repo, without cloning anything.

1. Select the student's message and press **⌥G**, or choose **GitHub Debug…** in the menu bar. The message is filled in automatically. If the student pasted a repo link in the message or earlier in the chat, the URL is filled in too.
2. Paste or confirm the repository URL. You can optionally add file names you suspect, like `Navbar.jsx` or `src/styles.css`.
3. Click **Analyze Repository** or press ⌘↩. PastePilot reads only the relevant files and shows:
   - the likely cause and the suggested fix
   - the file and line, with a short snippet taken from the real file
   - a **High**, **Medium** or **Low** confidence rating
4. It then writes the reply in your normal style, using your writing style, mode, knowledge base, examples and conversation memory. You can edit it.
5. Click **Paste Reply** or press ⌘⇧↩. The window closes, and the reply is pasted into your support chat without being sent. The rewrite bar and send detection work as usual.

**How files are chosen:**
1. **Names you typed** are resolved, so "navbar" finds `src/components/Navbar.jsx`.
2. **Filename search** matches the issue text. "About component" finds `About.jsx`, `About/index.tsx` and `components/About.jsx`.
3. **Topic files** are added. Routing issues bring in the App or router file, dependency errors bring in `package.json`, build issues bring in the Vite or webpack config, and a blank page brings in `index.html` and the main entry.
4. **One level of imports** is followed: the component's stylesheet for styling issues, and imported modules named in the issue.
5. **The AI can ask for up to 4 more files, twice at most.** Everything is capped:

| Limit | Value |
|---|---|
| Files per analysis | 12 |
| Characters per file | 20,000 |
| Characters in total | 70,000 |

**When it isn't sure,** it says so with "No confident diagnosis", and the reply asks the student for the missing information instead of guessing.

**Safety:**
- **Read-only.** It never pushes, never opens pull requests, and never runs code or npm scripts. Analysis is static only.
- **Repo content is untrusted.** The analyzer is told to ignore any instructions in the code or the student's message. The reply writer only receives the structured diagnosis, never the raw files.

**GitHub usage:**
- **One REST API call per analysis** fetches the file tree. Asking for `HEAD` resolves the default branch, so there's no extra lookup.
- **File contents come from `raw.githubusercontent.com`,** which doesn't count toward GitHub's limit of 60 anonymous requests per hour.
- **Caching.** Trees are cached for 3 minutes. Files are cached by their content hash, so a re-analysis is instant and never stale.
- **Private repos.** The client already accepts a token, but this version doesn't expose it, so private repositories show "Repository not found".

## Prompt pipeline

```
[system]    style + fixed rules + measured style profile + custom instructions   (cached, identical every time)
[system]    mode instructions + matching knowledge-base entries + matching examples
[user/asst] recent conversation turns (within the size limit)
[user]      current student message
(rewrites add: [assistant] current draft, [user] rewrite instruction)
```

## Project structure

```
src/                         React Settings window
  App.tsx                    sidebar shell + Save
  api.ts                     typed wrappers for Rust commands
  components/common.tsx      shortcut recorder, confirm button, toggles
  sections/                  General, AI, Style, Examples, Knowledge, Memory, Shortcuts
  debug/DebugApp.tsx         GitHub Debug window (same bundle, chosen by window label)
src-tauri/src/
  flow.rs        ★ generate + rewrite workflows (never presses Return)
  memory.rs      conversation scoping, de-duplication, history size limit
  modes.rs       classifier + default mode instructions
  retrieval.rs   FTS5 search for knowledge + examples, style profile
  debug.rs       GitHub Debug Mode: file planning, analysis, diagnosis, reply, paste
  github.rs      read-only GitHub client (tree API + raw files), URL parsing, cache
  repo_search.rs file filtering, filename/path search, topic files, import resolution
  rewrite.rs     rewrite actions, find-and-replace the pasted reply
  sent_watch.rs  saves the reply you actually sent (with edits) to memory
  prompt.rs      prompt pipeline, reply cleanup
  db.rs          SQLite schema, migrations, queries
  import.rs      bulk import parsers
  openai.rs      streaming Chat Completions client
  selection.rs   reads the selection
  settings.rs    settings.json
  state.rs       shared state (settings, DB, cached prompt, last reply)
  shortcut.rs    global hotkeys and dispatch
  tray.rs        menu bar menu
  commands.rs    Settings UI commands
  keychain.rs    API key in Keychain
  windows.rs     Settings window
  macos/
    ax.rs             Accessibility: selection, editable check, URL/title, text ranges
    action_bar.rs     native rewrite bar (non-activating panel)
    hud.rs            native status pill
    focus_tracker.rs  remembers the last text input per app
    keys.rs, pasteboard.rs, apps.rs
```

## Local storage

All data stays on your Mac, in `~/Library/Application Support/com.pastepilot.app/`:

| File | Contents |
|---|---|
| `pastepilot.db` | SQLite: conversations, messages, reply examples, knowledge base |
| `settings.json` | settings (no secrets) |
| Keychain item `com.pastepilot.app` | OpenAI API key |

The schema is versioned with `PRAGMA user_version` and migrations run automatically on launch. It lives in `src-tauri/src/db.rs`.

```sql
conversations (id, scope_key, title, closed, created_at, updated_at)
messages      (id, conversation_id → conversations ON DELETE CASCADE,
               role 'student'|'agent', content, mode, created_at)
reply_examples(id, student_message, reply, category, created_at, updated_at)
  + reply_examples_fts  (FTS5, porter stemming, kept in sync by triggers)
knowledge_base(id, title, content, category, tags, enabled, created_at, updated_at)
  + knowledge_fts       (FTS5, porter stemming, kept in sync by triggers)
```

Settings has separate delete buttons for all conversation history, all reply examples, and the whole knowledge base.

## Setup

### Prerequisites (one time)

| Tool | Install |
|---|---|
| macOS 11+ | |
| Xcode Command Line Tools | `xcode-select --install` |
| Rust (stable) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js 20.19+ | nodejs.org, nvm, or Homebrew |

### Commands

```bash
npm install                 # frontend packages
npm run app:dev             # run in development
npm run app:build           # → src-tauri/target/release/bundle/macos/PastePilot.app
npx tauri build             # .app + .dmg
cd src-tauri && cargo test  # unit tests
```

### Install or upgrade

Quit PastePilot from the menu bar first, then run:

```bash
rm -rf /Applications/PastePilot.app
cp -R src-tauri/target/release/bundle/macos/PastePilot.app /Applications/
open /Applications/PastePilot.app
```

After each new build, expect two things:
- **Keychain prompt.** macOS asks whether PastePilot may read its Keychain item. Enter your Mac login password and click **Always Allow**.
- **Accessibility may stop working.** If it does, remove PastePilot from System Settings → Privacy & Security → Accessibility, add it again, and restart the app. You can also reset it with `tccutil reset Accessibility com.pastepilot.app`.

### Packages

- **npm:** `react`, `react-dom`, `@tauri-apps/api`, `@tauri-apps/cli`, `vite`, `@vitejs/plugin-react` and `typescript`.
- **Rust:** `tauri`, `tauri-plugin-global-shortcut`, `tauri-plugin-autostart`, `reqwest`, `rusqlite` (bundled SQLite with FTS5), `security-framework`, `core-graphics` and `objc2-app-kit`.

### Environment variables

None are required.

| Variable | Purpose |
|---|---|
| `OPENAI_API_KEY` | Development fallback when no key is in the Keychain. |

## Testing

**Unit tests** cover the parts that don't need macOS permissions:

```bash
cd src-tauri && cargo test
cd src-tauri && cargo test live_ -- --ignored   # live check against a real public GitHub repo
```

They cover memory scoping and isolation, the history size limit, de-duplication, the npm "I sent it above" scenario, mode classification, knowledge and example relevance, billing guardrails, import formats, in-place rewrite matching, the stream parser and migrations.

**Manual test in the real app:**
1. **Memory.** In a support chat, select the message "My npm install isn't working." and press ⌥R. Edit the pasted reply and send it. Settings → Conversation Memory should show your edited version. Then select "I sent it above." and press ⌥R. The reply should understand the context.
2. **Isolation.** Open a different ticket and press ⌥R. Settings shows a new conversation with no earlier turns.
3. **Modes.** Select "Can I get a refund?". The pill says "Billing". With no refund entry in the knowledge base, the reply must not state a policy.
4. **Knowledge base.** Add a "Refund policy" entry and repeat step 3. The reply now uses it.
5. **Rewrites.** After a reply is pasted, press ⌥1. The reply in the box is replaced with a shorter version, and nothing is sent.
6. **Examples.** Edit a pasted reply, click ★ Save, and check that it appears under Reply Examples.
7. **GitHub Debug.** Push a tiny React app with a bug to a public repo, for example `display: none` on `.navbar` inside a mobile media query. Select a message like "my navbar disappears on my phone" and press ⌥G. Paste the URL and click Analyze. It should point to the CSS line with High or Medium confidence. Click Paste Reply and check that the reply lands in the chat box unsent.
8. **GitHub errors.** Try a misspelled repo, which shows "Repository not found". Try a file name that doesn't exist, which adds the note "That file could not be found".

## Known limitations

- **Keyword search, not semantic.** "Money back" only finds a refund entry through the billing word list. True semantic search would need an embeddings call before every reply, adding roughly 0.2 to 0.5 seconds. The search layer is isolated in `retrieval.rs` if you want to add it later.
- **Detecting a send relies on the reply box emptying.** If you clear the box yourself with select-all and delete, that also counts as "sent". If the text can't be read, the generated draft is kept.
- **Rich editors that reformat text heavily** may stop in-place rewrites from finding the old reply. You then get "New version copied" and paste it yourself.
- **Firefox** exposes less through Accessibility than Chrome or Safari, so remembering the reply box and in-place rewrites work best in Chromium browsers and Safari.
- **GitHub Debug only reads public repositories** and does static analysis, so it can't see runtime errors, environment variables or anything that isn't pushed. Large repositories may only be partly listed by GitHub.
- **Without sign-in, GitHub allows 60 file-list requests per hour.** That's about 60 analyses of different repos, and re-analyzing a repo within 3 minutes uses the cache.
- **Unsigned builds** need the Keychain and Accessibility re-approval described above after every rebuild. An Apple Developer ID removes this.
