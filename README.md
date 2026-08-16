# XP Static Wiki

A Dioxus fullstack wiki written in Rust. Pages are Markdown files stored in a local Git repository, page edits are committed with the authenticated user as author, and revision diffs are generated with `git2`.
The page sidebar is sorted by page creation time, oldest first.

## Run

Install the Dioxus CLI if needed:

```sh
cargo install dioxus-cli
```

Configure OAuth2 in `.env`:

```sh
cp .env.example .env
```

The app supports GitHub, Google, and Discord. Configure whichever provider credentials you want enabled in `.env`; the UI only shows login options for providers that have both client ID and client secret set.
The server loads `.env` from the project root by default. Set `XP_WIKI_ENV_FILE=/absolute/path/to/.env` before starting the server if you want to use a different file.
Set `XP_WIKI_UI_URL` when the OAuth server should send the browser to a UI hosted at a different URL after a successful login. If it is unset, the server redirects to `/`.
Use the same host in the browser and OAuth callback URL, for example `127.0.0.1` everywhere or `localhost` everywhere. Browsers do not share cookies between those hostnames.
Login sessions are stored in `XP_WIKI_SESSION_FILE`, defaulting to `wiki-data/sessions.json`, so OAuth login cookies survive server restarts until they expire.

Page templates are Markdown files in `wiki-data/templates`. The first `# Heading` is used as the template name in the editor, and the remaining Markdown is copied into new page drafts. Templates can include `{{title}}` and `{{slug}}` placeholders, which are replaced when an editor applies the template.

Pages can declare categories in a leading front matter block. Category names are normalized like slugs, so `test page`, `test_page`, and `test-page` all match `test-page`:

```markdown
---
categories: [test-page, npc]
promoted: false
---

# Example Page
```

Pages are promoted into the sidebar by default. Set `promoted: false`, or clear the Promoted checkbox in the editor, to hide a page from the sidebar.

Use `{{category:test-page}}` in page Markdown to render a list of pages in that category. Bare category shortcodes such as `{{test_page}}` are also supported; `{{title}}` and `{{slug}}` remain reserved for templates.

The page editor includes Builder and Markdown modes. Builder mode inserts common page blocks through form controls, including sections, paragraphs, images, table-of-contents blocks, callouts, infoboxes, item cards, and NPC cards. NPC card portraits can be selected from a popup media picker. The generated content is still Markdown, so pages continue to save as local `.md` files with normal Git history.

Pages can include a MediaWiki-style infobox with a fenced `infobox` block. The `infocard` and `info-card` aliases work too. Values are escaped when rendered, so editors should write plain text rather than HTML:

````markdown
```infobox
title: Nikola Tesla
image: tesla.jpeg
caption: Tesla around 1890
Born: 10 July 1856
Known for: AC power
```
````

For local wiki media, put image files in `wiki-data/media` or the repo-level `media` folder and use `image: filename.ext`. The `/media` route looks in those folders first and then in the app `assets` directory, so existing bundled images can use the same filename style. You can also use an `https://` URL.

The Media tab browses files under `wiki-data/media`. Editors and admins can create folders, upload image or video files up to 50 MiB each, drag media files onto folder tiles to move them, and delete files or folders with a confirmation step. Uploads, moves, and deletes are saved into Git history and served from `/media/path/to/file.ext`.

Editors and admins can use `Export HTML` to generate a static copy of the wiki at `wiki-data/export/latest`. The export writes root-level page HTML files, `assets/tailwind.css`, and copied media, and the running app serves the latest export from `/exports/latest/index.html`.

Pages can include a table of contents with a fenced `toc` block. It adds heading anchors automatically and includes `##` through `######` headings by default:

````markdown
```toc
title: Contents
min-depth: 2
max-depth: 4
ordered: false
```
````

Use `title: false` to hide the title. `ordered: true` renders a numbered list.

Markdown components can also be declared as static JSON plugin manifests in `wiki-data/components`. The server seeds `callout.json`, `infobox.json`, and `item-card.json`, and the browser loads these manifests so page view and editor preview render the same component set. Built-in manifests are compiled into the app as fallbacks for static builds.

Example `wiki-data/components/callout.json` manifest:

```json
{
  "name": "Callout",
  "fence": "callout",
  "aliases": ["note"],
  "wrapper_tag": "aside",
  "wrapper_class": "callout",
  "aria_label": "Callout",
  "fields": [
    { "key": "title", "type": "text", "tag": "div", "class": "callout-title" },
    { "key": "image", "type": "image", "class": "callout-image", "width": 320, "height": 180 },
    { "key": "body", "aliases": ["text"], "type": "text", "tag": "p", "class": "callout-body", "repeatable": true },
    { "key": "items", "type": "list", "split": "|", "wrapper_tag": "ul", "wrapper_class": "callout-list", "item_tag": "li" }
  ],
  "unknown_fields": {
    "class": "markdown-component-fields",
    "row_class": "markdown-component-row",
    "label_class": "markdown-component-label",
    "value_class": "markdown-component-value"
  }
}
```

Editors can use that component in Markdown:

````markdown
```callout
title: Note
body: This component is declared with JSON rather than Rust.
items: Static | Safe | Reusable
Status: Draft
```
````

Declarative fields support `text`, `image`, `list`, `key_value_list`, and `class_marker`. Image fields can define numeric `width` and `height` attributes, or a square `size` value that sets both. Manifests can also define a safe `layout` with `container`, `field`, and `remaining_fields` nodes for nested component markup. Text is escaped, image paths are restricted to safe local media paths or `http(s)` URLs, and manifest tags/classes are sanitized before rendering.

Pages can also include RPG-style item panels with a fenced `item-card` block. The `item`, `itembox`, and `item-box` aliases work too:

````markdown
```item-card
title: Amulet of Loyalty
icon: necklace
tags: MAGIC, UNIQUE
line: Slot: NECK
line: INT: +5 | WIS: +5
line: Mana: +30
description: A simple pendant carrying Lady Elana's final vow, clasped from Roon Torvald's lifeless chest.
line: Weight: 0.1 | Size: SMALL
line: Class: ALL
line: Race: ALL
```
````

Roles are managed with `role-system` using its filesystem backend at `XP_WIKI_ROLE_FILE`. The app bootstraps `admin`, `editor`, and `viewer` roles there. Authenticated users are tracked in `XP_WIKI_USER_FILE`, and admins can assign `admin`, `editor`, `viewer`, or `none` from the Users tab. The Settings tab is locked to admins and shows the runtime storage, auth, role, and upload-limit configuration.

Users listed in `XP_WIKI_ADMIN_USERS`, `XP_WIKI_EDITOR_USERS`, or `XP_WIKI_VIEWER_USERS` are locked by `.env` and cannot be changed from the UI. `XP_WIKI_DEFAULT_ROLE` is used for authenticated users without an explicit managed role.

Run the web app:

```sh
npm install
npm run tailwind:watch
npm run dev:web
```

`dev:web` passes `--open false` to Dioxus. If your desktop opener is configured correctly and you want Dioxus to open the browser automatically, run `npm run dev:web:open` instead.

Run the desktop app against the same server:

```sh
export XP_WIKI_SERVER_URL="http://127.0.0.1:8080"
npm run dev:desktop
```
