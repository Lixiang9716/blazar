//! Demo timeline entries for visual testing and playback.

use crate::chat::model::TimelineEntry;

/// Demo timeline entries for visual testing (BLAZAR_DEMO env var).
pub fn demo_timeline() -> Vec<TimelineEntry> {
    demo_playback_script().into_iter().take(3).collect()
}

/// Full demo playback script — one entry per second when triggered by "1".
/// Covers every entry kind: hint, warning, message, user_message,
/// tool_use (with diff details), bash, thinking, code_block.
pub fn demo_playback_script() -> Vec<TimelineEntry> {
    vec![
        // --- System initialization ---
        TimelineEntry::hint(
            "No blazar instructions found. Run /init to generate a blazar-instructions.md file.",
        ),
        TimelineEntry::warning("Failed to load 2 skills. Run /skills for more details."),
        TimelineEntry::response(
            "Environment loaded: 1 MCP server, 4 plugins, 12 skills, 3 agents",
        ),

        // --- User starts a task ---
        TimelineEntry::user_message("Fix the login page — the submit button doesn't work on mobile."),

        // --- Assistant thinks ---
        TimelineEntry::thinking(
            "The user wants to fix the mobile submit button on the login page. \
             I should look at the login component and its CSS first, \
             then check for touch event handlers.",
        ),

        // --- Assistant responds with analysis ---
        TimelineEntry::response(
            "I'll investigate the login page. Let me check the component and its styles.",
        ),

        // --- Read file ---
        TimelineEntry::tool_use(
            "Read",
            "src/components/LoginForm.tsx",
            0,
            0,
            "Reading login form component",
        )
        .with_details(
            "export function LoginForm() {\n\
             \x20 const [email, setEmail] = useState('');\n\
             \x20 const handleSubmit = (e: MouseEvent) => {\n\
             \x20   e.preventDefault();\n\
             \x20   submitLogin(email, password);\n\
             \x20 };\n\
             \x20 return (\n\
             \x20   <button onClick={handleSubmit}>Sign In</button>\n\
             \x20 );\n\
             }",
        ),

        // --- Bash: run tests ---
        TimelineEntry::bash(
            "npm test -- --grep 'LoginForm'",
            "FAIL src/components/LoginForm.test.tsx\n  ✕ submit fires on touch (24ms)\n  ✓ renders email input (3ms)\n  ✓ shows validation error (5ms)",
        )
        .with_details(
            "$ npm test -- --grep 'LoginForm'\n\n\
             FAIL src/components/LoginForm.test.tsx\n\
             \x20 ● submit fires on touch\n\
             \x20   expect(mockSubmit).toHaveBeenCalled()\n\
             \x20   Expected: called\n\
             \x20   Received: not called\n\n\
             Tests: 1 failed, 2 passed, 3 total\n\
             Time:  1.842s",
        ),

        // --- Assistant analyzes ---
        TimelineEntry::response(
            "Found the bug: `onClick` only fires on mouse click, not touch. \
             Mobile Safari requires `onPointerDown` or a combined handler. \
             I'll fix the event handler and update the CSS for touch targets.",
        ),

        // --- Edit file (with diff) ---
        TimelineEntry::tool_use(
            "Edit",
            "src/components/LoginForm.tsx",
            5,
            3,
            "Switch onClick to onPointerDown for mobile support",
        )
        .with_details(
            "  const handleSubmit = (e: MouseEvent) => {\n\
             -   e.preventDefault();\n\
             -   submitLogin(email, password);\n\
             - };\n\
             + const handleSubmit = (e: React.PointerEvent | React.MouseEvent) => {\n\
             +   e.preventDefault();\n\
             +   submitLogin(email, password);\n\
             + };\n\
             \n\
             - <button onClick={handleSubmit}>Sign In</button>\n\
             + <button onPointerDown={handleSubmit} style={{ touchAction: 'manipulation' }}>\n\
             +   Sign In\n\
             + </button>",
        ),

        // --- Edit CSS ---
        TimelineEntry::tool_use(
            "Edit",
            "src/styles/login.css",
            8,
            0,
            "Add mobile touch-friendly button styles",
        )
        .with_details(
            "+ .login-btn {\n\
             +   min-height: 44px;\n\
             +   min-width: 44px;\n\
             +   touch-action: manipulation;\n\
             +   -webkit-tap-highlight-color: transparent;\n\
             +   cursor: pointer;\n\
             +   padding: 12px 24px;\n\
             +   font-size: 16px; /* prevents iOS zoom */\n\
             + }",
        ),

        // --- Run tests again ---
        TimelineEntry::bash(
            "npm test -- --grep 'LoginForm'",
            "PASS src/components/LoginForm.test.tsx\n  ✓ submit fires on touch (8ms)\n  ✓ renders email input (3ms)\n  ✓ shows validation error (4ms)",
        )
        .with_details(
            "$ npm test -- --grep 'LoginForm'\n\n\
             PASS src/components/LoginForm.test.tsx\n\
             \x20 ✓ submit fires on touch (8ms)\n\
             \x20 ✓ renders email input (3ms)\n\
             \x20 ✓ shows validation error (4ms)\n\n\
             Tests: 3 passed, 3 total\n\
             Time:  1.204s",
        ),

        // --- Code block ---
        TimelineEntry::code_block(
            "typescript",
            "// Key change: onPointerDown works on both mouse and touch\n\
             <button\n\
             \x20 onPointerDown={handleSubmit}\n\
             \x20 style={{ touchAction: 'manipulation' }}\n\
             >\n\
             \x20 Sign In\n\
             </button>",
        ),

        // --- Lint check ---
        TimelineEntry::bash(
            "npx eslint src/components/LoginForm.tsx",
            "No lint errors found.",
        ),

        // --- Summary response with markdown ---
        TimelineEntry::response(
            "**Fixed.** The mobile submit button now works correctly.\n\n\
             ### Changes\n\
             - Replaced `onClick` with `onPointerDown` for cross-device support\n\
             - Added `touchAction: 'manipulation'` to prevent 300ms delay\n\
             - CSS: minimum 44×44px touch target (WCAG 2.5.5)\n\
             - Font size 16px to prevent iOS auto-zoom\n\n\
             All 3 tests pass. The fix covers iOS Safari, Android Chrome, \
             and desktop browsers.",
        ),

        // --- Another user request ---
        TimelineEntry::user_message("Can you also add a loading spinner to the button?"),

        // --- Thinking ---
        TimelineEntry::thinking(
            "The user wants a loading state. I'll add a spinner component \
             that shows during the async login request, disabling the button \
             to prevent double-submit.",
        ),

        // --- Tool use: create new file ---
        TimelineEntry::tool_use(
            "Create",
            "src/components/Spinner.tsx",
            12,
            0,
            "Create reusable spinner component",
        )
        .with_details(
            "+ import React from 'react';\n\
             + import './spinner.css';\n\
             +\n\
             + interface SpinnerProps {\n\
             +   size?: number;\n\
             +   color?: string;\n\
             + }\n\
             +\n\
             + export function Spinner({ size = 16, color = 'white' }: SpinnerProps) {\n\
             +   return <span className=\"spinner\" style={{ width: size, height: size, borderColor: color }} />;\n\
             + }",
        ),

        // --- Final summary ---
        TimelineEntry::response(
            "**Done.** Added a `<Spinner>` component that shows during login. \
             The button is disabled while loading to prevent double-submits.\n\n\
             Run `/commit` to stage these changes.",
        ),
    ]
}
