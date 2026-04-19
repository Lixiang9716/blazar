import { joinSession } from "@github/copilot-sdk/extension";

const BLOCKED_COMMANDS = new Map([
  ["cargo fmt", "just fmt"],
  ["cargo clippy", "just lint"],
  ["cargo test", "just test"],
  ["cargo nextest run", "just test"],
  ["cargo llvm-cov", "just cov"],
  ["cargo deny", "just audit"],
]);

function normalizeCommand(command) {
  return typeof command === "string" ? command.trim().replace(/\s+/g, " ") : "";
}

function findBlockedCommand(command) {
  return [...BLOCKED_COMMANDS.entries()].find(([prefix]) =>
    command.startsWith(prefix),
  );
}

const additionalContext =
  "Use the repository engineering workflow. For formatting, linting, tests, coverage, and audit actions, use the shared commands just fmt, just lint, just test, just cov, and just audit instead of raw cargo commands.";

await joinSession({
  hooks: {
    onUserPromptSubmitted: async () => ({
      additionalContext,
    }),

    onPreToolUse: async (input) => {
      if (input.toolName !== "bash") {
        return;
      }

      const command = normalizeCommand(input.toolArgs?.command);
      if (!command.startsWith("cargo ")) {
        return;
      }

      const blockedCommand = findBlockedCommand(command);
      if (!blockedCommand) {
        return;
      }

      const [blockedPrefix, suggestedCommand] = blockedCommand;
      return {
        permissionDecision: "deny",
        permissionDecisionReason: `Use ${suggestedCommand} instead of raw '${blockedPrefix}'.`,
      };
    },
  },
});
