# Message Layout + Group Chat Indicator Design

**Date:** 2026-03-10

## Features

### 1. Right-aligned outgoing messages

Outgoing messages (where `msg.is_outgoing == true`) are rendered right-justified inside the message view panel.

**Header line:** rendered as `timestamp  SenderName` and padded with leading spaces to push it to the right edge of the panel width.

**Content lines:** each line padded with leading spaces so the text ends at the right edge of the panel width.

Incoming messages are unchanged (left-aligned as today). Colors stay the same (green sender, white text for outgoing).

**File:** `src/tui/widgets/message_view.rs`

### 2. Group chat `[GP]` indicator in chat list

In the chat list, if `chat.is_group == true`, render `[GP]` in **Magenta** instead of the platform tag (`[WA]`/`[TG]`/`[SL]`).

`[GP]` takes priority over the platform tag, the same way `[NL]` (newsletter, Cyan) does today. The precedence order becomes:

1. `[NL]` in Cyan — if `chat.is_newsletter`
2. `[GP]` in Magenta — else if `chat.is_group`
3. Platform tag in DarkGray — otherwise

**File:** `src/tui/widgets/chat_list.rs`

## Non-changes

- No new fields on `UnifiedMessage` or `UnifiedChat` — `is_group` already exists on `UnifiedChat`
- No color changes to message bubbles inside group chats
- No changes to any provider code
