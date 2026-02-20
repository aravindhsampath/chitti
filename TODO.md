# Chitti TODO List

## Architecture & Engineering
- [ ] **Internal Type Safety**: Move from passing raw `String` and `serde_json::Value` between modules to using strongly typed structs for internal communication. This will reduce serialization overhead and catch logic errors at compile time.
- [ ] **Persistent History**: Implement a local SQLite or file-based storage for multi-session conversation memory.
- [ ] **Encryption**: Securely store API keys and sensitive environment variables using macOS Keychain.

## Tools & Capabilities
- [ ] **File Editor Tool**: Add a tool that allows the model to perform structured edits (line-based or token-based) on local files.
- [ ] **Browser Automation**: Allow Chitti to use Puppeteer or a similar tool to browse the web for real-time information beyond search results.

## Communication Bridges
- [ ] **Raycast Extension**: Build a Chitti frontend for the Raycast launcher.
- [ ] **WhatsApp/Signal Bridge**: Implement a webhook-based bridge for remote interaction.
