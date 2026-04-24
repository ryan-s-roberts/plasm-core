# Outlook (Microsoft Graph) — Plasm CGS

Wave 1 in this tree models the signed-in mailbox as three agent-facing entities: `MailFolder`, `Message`, and `Attachment`. The focus is mailbox navigation and triage: list folders, descend into child folders, list messages across the mailbox or within a folder, inspect a full message, list attachments, download attachment content, and perform basic mailbox state changes (`message_update`, `message_move`, `message_send`, `message_delete`). Later waves should add calendar/event surfaces, richer composition flows, and conversation-level abstractions only when they compress the domain rather than mirror Graph RPC paths.

Docs consulted:
- <https://learn.microsoft.com/en-us/graph/api/user-list-mailfolders?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/mailfolder-list-childfolders?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/mailfolder-get?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/user-list-messages?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/mailfolder-list-messages?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/message-get?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/message-update?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/message-move?view=graph-rest-1.0>
- <https://learn.microsoft.com/en-us/graph/api/message-send?view=graph-rest-1.0>
- <https://learn.microsoft.com/en-us/graph/api/message-delete?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/message-list-attachments?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/api/attachment-get?view=graph-rest-1.0&tabs=http>
- <https://learn.microsoft.com/en-us/graph/permissions-reference>

## Auth and permissions

- Scheme: `bearer_token` via `MICROSOFT_GRAPH_ACCESS_TOKEN`.
- Flow assumption: delegated Microsoft Graph user token for the signed-in mailbox. This catalog intentionally uses `/me/...` paths, so application-only client-credentials tokens are not the default wave-1 path.
- Practical least privilege:
  - Read-only navigation: `Mail.ReadBasic` or `Mail.Read`
  - Full message bodies and attachment content: `Mail.Read`
  - Message state changes, moves, draft send, delete: `Mail.ReadWrite`
- If you later want an application-only mailbox catalog, author a separate `/users/{id}`-scoped CGS slice rather than overloading this one.

```bash
export MICROSOFT_GRAPH_ACCESS_TOKEN="..."
cargo run -p plasm-agent --bin plasm-cgs -- \
  --schema apis/outlook \
  --backend https://graph.microsoft.com \
  message query
```

## Entity graph

- `MailFolder`
  - root folders via `mail_folder_query`
  - child folders via `mail_folder_child_query`
  - messages via `message_folder_query`
- `Message`
  - mailbox-wide messages via `message_query`
  - detail via `message_get`
  - attachments via `attachment_query`
  - triage writes via `message_update`, `message_move`, `message_send`, `message_delete`
- `Attachment`
  - attachment metadata via `attachment_query`
  - attachment content via `attachment_get`

## Known limitations

- Microsoft Graph mailbox collections commonly paginate with `@odata.nextLink` as an absolute URL in the response body. Plasm's current HTTP pagination machinery advances query/body params or `Link` headers, not "follow this whole URL from JSON", so the list capabilities in this wave return the first page only. The mappings pin `$top=100` to make that first page as useful as possible.
- `Attachment` detail still requires an explicit `messageId` scope parameter on `attachment_get`. The relation `Message.attachments` lists attachment rows cleanly, but the current runtime does not automatically carry the parent message identity into a later attachment detail call.

## Validation note

This tree was validated with:

```bash
cargo run -p plasm-cli --bin plasm -- schema validate apis/outlook
cargo run -p plasm-eval -- coverage --schema apis/outlook --cases apis/outlook/eval/cases.yaml
```

`cargo run -p plasm-cli -- validate --schema apis/outlook --spec ...` was not run in this change because no minimal, maintainable Microsoft Graph OpenAPI slice is checked into the repo yet. A full Graph description would be disproportionately large and noisy for this wave; schema validation plus eval coverage were the clean validation path for today.
