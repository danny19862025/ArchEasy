# Ask AI — ChatGPT, Gemini and Claude

Click the **✨ Ask AI** tab at the top, next to **Snapshots & backups…**, then choose ChatGPT, Gemini or Claude from Provider. Choose **Add a ChatGPT account…**, click **Continue with ChatGPT**, and complete OpenAI login and consent in your system browser. Return to ArchEasy, select one of the models available to your account, type a question, check the send-consent box, and click **Send**. On later launches, choose your saved account and click **Load models**. Continue with ChatGPT reconnects the selected registration; the Add account option starts a separate registration. Account labels include a registration suffix to distinguish workspaces with the same email.

The ChatGPT option uses ChatGPT-plan authorization, not an API key. Eligibility, models and allowance are controlled by OpenAI; a successful login does not guarantee plan-use permission. If OpenAI declines access, ArchEasy shows the error and does not fall back to API billing or copy Codex/ChatGPT credentials. This feature requires Python 3 and `python-cryptography`; the latter is an optional Arch package dependency and is not installed automatically.

## Gemini and Claude

Select **Gemini** or **Claude**, enter your own provider API key in the masked key box, then click **Load models**. Choose an available model, type a question and check the provider-specific send-consent box before sending. **Get API key** opens the relevant provider's key-management page. Never paste keys into the chat itself.

Gemini uses the Google AI Studio/Gemini API; Claude uses the Anthropic API. These are API connections with their own account permissions, quotas, data terms and possible usage charges; consumer app subscriptions are not reused. There is no Gemini/Claude consumer OAuth implementation, token borrowing, CLI wrapper or API-billing fallback from ChatGPT.

Gemini/Claude keys are held only in the masked entry/process memory while that provider is selected. They are never saved to disk, placed on command lines, included in chat messages, or sent in URLs. They are passed through the private worker pipe and sent only in the selected provider's authentication header. Switching provider clears the key, chat, model catalog and consent, so history and credentials do not cross providers. Changing a key also clears that provider's chat and loaded models. Re-enter a key after switching back or restarting.

The model picker comes from the provider's own model-list endpoint. Gemini lists only models supporting generateContent. ChatGPT replies stream; Gemini/Claude replies appear when their text request completes. Truncated or blocked replies are shown as errors, not successful chat messages. Chat has no tools or execution capability on any provider.

## Privacy and operation

Only text entered into this chat, its previous successful messages for the selected provider, and ArchEasy's fixed assistant instructions are sent. There is no automatic collection of packages, logs, hardware information, files, clipboard contents, or existing ChatGPT conversations. Paste an error manually after removing secrets. The consent box starts unchecked. Replies stream as plain text and may contain commands for you to review and run yourself: the chat has no shell, file, installation or other operation tools.

Chat history is kept only in memory while ArchEasy runs. New chat clears it, and changing accounts or signing out clears it. Selecting New chat does not delete any provider-side service/security records. ChatGPT requests use the public Responses API with `store: false` and `stream: true`. Gemini uses generateContent and Claude uses Messages; those providers apply their own API data policies. Cancel stops the local request; processing may already have consumed allowance. Requests are not automatically retried.

ChatGPT account registrations, host ID, tokens and expiry data live under `$XDG_CONFIG_HOME/archeasy/ask-ai/` (normally `~/.config/archeasy/ask-ai/`). That directory is owner-only (0700); `accounts.json` is atomically written with owner-only (0600) access. These are sensitive plaintext credentials protected by filesystem permissions, not an encrypted keyring. Never share or commit them. A file lock serializes refresh rotation. Sign out attempts remote refresh-token revocation, then removes tokens locally while retaining the account/client mapping and stable host ID for future sign-in. If remote revocation cannot be confirmed, the app says so and directs you to disconnect it in ChatGPT Settings.

The default `.config/archeasy/ask-ai` directory is excluded from ArchEasy’s unencrypted cloud/portable backups. Custom XDG locations need equivalent exclusion in external backup tools. An interrupted first-time exchange keeps an unverified registration labeled Finish signing in; select it and Continue with ChatGPT to retry without creating another registration. Pending registrations cannot send AI requests.

The native sign-in flow starts a bounded HTTP listener on 127.0.0.1 before opening the browser, uses a fresh state, nonce and S256 PKCE verifier, validates the callback/client binding, and exchanges the code using the issued client ID (never the dynamic registration placeholder). RSA/EC ID-token signatures are verified using `cryptography` against OpenAI's JWKS, followed by issuer, audience, expiry, issued-at and nonce checks. Unknown algorithms or identities fail closed. Network endpoints are fixed to OpenAI; authenticated redirects are rejected. Errors do not display tokens or raw provider responses.

## Development and verification

`src/ask_ai.rs` provides the native GTK page and a cancellable worker. `scripts/ask-ai.py` is embedded into the binary and communicates through private stdin/stdout pipes as JSON lines; it does not need to be installed separately. No credentials are passed on command lines. The account-specific model catalog supplies both display names and request slugs; no model is hardcoded.

Run `python3 tests/ask_ai_test.py` with `python-cryptography` available. These tests verify signed identity rejection, callback/state/client binding, private storage, symlink refusal, refresh rotation, model selection, streaming completion, request privacy and sign-out. One test uses a real private loopback listener with mocked OpenAI endpoints; it needs local socket permission. The tests never log into a real account or consume ChatGPT allowance.

A live end-to-end login and reply require the owner's browser consent and an eligible account. Do not claim live connectivity from fixture tests. Existing ignored graphical tests have their own separate audited runner; this feature does not modify that inventory or weaken those assertions.

## Official protocol references

- [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Accounts, refresh and sign-out](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)
- [Account models and streaming inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)

The integration follows the published preview flow; server eligibility and protocol availability can change independently of ArchEasy.

Provider references: [Gemini API keys](https://ai.google.dev/gemini-api/docs/api-key), [Gemini models](https://ai.google.dev/api/models), [Gemini generation](https://ai.google.dev/api/generate-content), [Claude Messages](https://platform.claude.com/docs/en/api/messages), [Claude model list](https://platform.claude.com/docs/en/api/models/list), [Claude authentication guidance](https://code.claude.com/docs/en/legal-and-compliance).
