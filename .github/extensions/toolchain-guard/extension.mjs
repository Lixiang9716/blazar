import { joinSession } from "@github/copilot-sdk/extension";

const BLOCKED_PREFIXES = [
  "cargo fmt",
  "cargo clippy",
  "cargo test",
  "cargo llvm-cov",
  "cargo deny",
];

const SUGGESTED_COMMANDS = new Map([
  ["cargo fmt", "just fmt-check"],
  ["cargo clippy", "just lint"],
  ["cargo test", "just test"],
  ["cargo llvm-cov", "just cov"],
  ["cargo deny", "just audit"],
]);

function normalizeCommand(command) {
  return typeof command === "string" ? command.trim().replace(/\s+/g, " ") : "";
}

function matchingPrefix(command) {
  return BLOCKED_PREFIXES.find((prefix) => command.startsWith(prefix));
}

const session = await joinSession({
  hooks: {
    onUserPromptSubmitted: async () => ({
      additionalContext:
        "Use the repository engineering workflow. For formatting, linting, tests, coverage, and audit actions, prefer the shared just commands instead of raw cargo commands.",
    }),

    onPreToolUse: async (input) => {
      if (input.toolName !== "bash") {
        return;
      }

      const command = normalizeCommand(input.toolArgs?.command);
      if (!command.startsWith("cargo ")) {
        return;
      }

      const blockedPrefix = matchingPrefix(command);
      if (!blockedPrefix) {
        return;
      }

      return {
        permissionDecision: "deny",
        permissionDecisionReason: `Use ${SUGGESTED_COMMANDS.get(blockedPrefix)} instead of raw '${blockedPrefix}'.`,
      };
    },
  },
});

session.on("session.shutdown", () => {});
