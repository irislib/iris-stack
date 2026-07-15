# Agent instructions

- Keep `README.md` and `stack.json` as public, durable architecture and
  ownership documentation.
- Keep the executable code a thin process-level integration lab, not a fourth
  substrate implementation.
- Do not copy FIPS, TCP/FIPS, or Hashtree framing, retry, or discovery logic here.
- Tests must use isolated loopback rendezvous ports and ordinary user privileges.
- Product gates must disable host-LAN discovery and pin public artifacts or exact
  public commits; local binary paths are runtime inputs only.
- Preserve separate OS processes for the external peer, rendezvous anchor, provider, and consumer roles.
