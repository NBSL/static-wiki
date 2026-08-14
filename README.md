# XP Static Wiki

A Dioxus fullstack wiki written in Rust. Pages are Markdown files stored in a local Git repository, page edits are committed with the authenticated user as author, and revision diffs are generated with `git2`.

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

Roles are managed with `role-system` using its filesystem backend at `XP_WIKI_ROLE_FILE`. The app bootstraps `admin`, `editor`, and `viewer` roles there. Authenticated users are tracked in `XP_WIKI_USER_FILE`, and admins can assign `admin`, `editor`, `viewer`, or `none` from the Users tab.

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
