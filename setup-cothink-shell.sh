#!/bin/bash
################################################################################
# Setup cothink-shell as system shell
# Run this script to install cothink-shell in /etc/shells and enable it
################################################################################

set -euo pipefail

SHELL_PATH="/usr/local/bin/cothink-shell"
COTHINK_SYSTEM_PATH="/home/adrian/cothink-system"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}cothink-shell Installation Setup${NC}"
echo ""

# Step 1: Check if source exists
if [[ ! -f "$COTHINK_SYSTEM_PATH/cothink-shell" ]]; then
    echo -e "${RED}✗ cothink-shell not found at $COTHINK_SYSTEM_PATH/cothink-shell${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Found cothink-shell source${NC}"

# Step 2: Make executable
chmod +x "$COTHINK_SYSTEM_PATH/cothink-shell"
echo -e "${GREEN}✓ Made cothink-shell executable${NC}"

# Step 3: Copy to /usr/local/bin
echo "Installing cothink-shell to $SHELL_PATH..."
if ! sudo cp "$COTHINK_SYSTEM_PATH/cothink-shell" "$SHELL_PATH"; then
    echo -e "${RED}✗ Failed to copy to $SHELL_PATH (need sudo)${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Installed cothink-shell${NC}"

# Step 4: Register in /etc/shells
if grep -q "^$SHELL_PATH" /etc/shells 2>/dev/null; then
    echo -e "${GREEN}✓ Already registered in /etc/shells${NC}"
else
    echo "Registering cothink-shell in /etc/shells..."
    echo "$SHELL_PATH" | sudo tee -a /etc/shells > /dev/null
    echo -e "${GREEN}✓ Registered in /etc/shells${NC}"
fi

# Step 5: Show how to use
echo ""
echo -e "${BLUE}Installation Complete!${NC}"
echo ""
echo "Usage:"
echo "  # Set as login shell"
echo "  chsh -s $SHELL_PATH"
echo ""
echo "  # Run directly"
echo "  cothink-shell"
echo "  cothink-shell --workers 4 --subagents 16"
echo "  cothink-shell --tasks task_definitions.json"
echo ""
echo "  # Pass arguments verbatim to cothink-system"
echo "  cothink-shell --version"
echo "  cothink-shell --help"
echo ""
echo -e "${BLUE}Current shells:${NC}"
grep "cothink" /etc/shells || echo "  (not yet registered)"
