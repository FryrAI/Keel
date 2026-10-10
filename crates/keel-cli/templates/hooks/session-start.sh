#!/bin/sh
# A session-start hook that exits non-zero shows the agent nothing.
# Its stderr never reaches the agent, so merge map diagnostics into stdout.
# Always succeed so the agent receives the map or the reason it is unavailable.
timeout 10 keel map --llm --cached 2>&1
exit 0
