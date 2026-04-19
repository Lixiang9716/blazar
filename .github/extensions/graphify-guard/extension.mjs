import { access, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { joinSession } from "@github/copilot-sdk/extension";

const sessionStates = new Map();

const BLOCKED_TOOLS = new Set([
  "bash",
  "glob",
  "rg",
  "task",
  "apply_patch",
  "github-mcp-server-search_code",
  "github-mcp-server-get_file_contents",
]);

// Bash commands that are safe without reading knowledge base first.
// These don't search or modify code — they manage git, processes, or infra.
const SAFE_BASH_PREFIXES = [
  "git ", "git\t",
  "ss ", "ps ", "kill ", "ls ", "pwd", "echo ",
  "cd ", "cat /tmp", "node /tmp",
  "find ", "head ", "tail ", "wc ",
  "cargo test", "cargo build", "cargo check", "cargo clippy",
  "just ", "make ",
  "npm ", "npx ",
  "cp ", "mv ", "mkdir ",
  "curl ", "wget ",
];

async function pathExists(targetPath) {
  try {
    await access(targetPath);
    return true;
  } catch {
    return false;
  }
}

async function findRepoRoot(startCwd) {
  let current = path.resolve(startCwd);

  while (true) {
    if (await pathExists(path.join(current, ".git"))) {
      return current;
    }

    const parent = path.dirname(current);
    if (parent === current) {
      return path.resolve(startCwd);
    }

    current = parent;
  }
}

async function findGuidePath(repoRoot) {
  const knowledgeBaseDir = path.join(repoRoot, "docs", "knowledge-base");
  if (!(await pathExists(knowledgeBaseDir))) {
    return undefined;
  }

  const entries = await readdir(knowledgeBaseDir, { withFileTypes: true });
  const candidates = [];

  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith(".md")) {
      continue;
    }
    if (entry.name === "README.md") {
      continue;
    }

    const filePath = path.join(knowledgeBaseDir, entry.name);
    const fileStats = await stat(filePath);
    candidates.push({
      filePath,
      mtimeMs: fileStats.mtimeMs,
      preferred:
        entry.name.toLowerCase().endsWith("-guide.md") ||
        entry.name.toLowerCase().includes("knowledge-base"),
    });
  }

  candidates.sort((left, right) => {
    if (left.preferred !== right.preferred) {
      return left.preferred ? -1 : 1;
    }
    return right.mtimeMs - left.mtimeMs;
  });

  return candidates[0]?.filePath;
}

function findStandardsPath(repoRoot) {
  return path.join(repoRoot, "docs", "knowledge-base", "blazar-coding-standards.md");
}

function toRepoRelative(repoRoot, absolutePath) {
  if (!absolutePath) {
    return undefined;
  }
  return path.relative(repoRoot, absolutePath).split(path.sep).join("/");
}

function normalizePath(targetPath) {
  if (!targetPath || typeof targetPath !== "string") {
    return undefined;
  }
  return path.resolve(targetPath);
}

async function resolveGraphifyContext(cwd) {
  const repoRoot = await findRepoRoot(cwd);
  const reportPath = path.join(
    repoRoot,
    "docs",
    "knowledge-base",
    "generated",
    "graphify",
    "GRAPH_REPORT.md",
  );

  if (!(await pathExists(reportPath))) {
    return {
      repoRoot,
      available: false,
    };
  }

  const graphJsonPath = path.join(
    repoRoot,
    "docs",
    "knowledge-base",
    "generated",
    "graphify",
    "graph.json",
  );
  const graphHtmlPath = path.join(
    repoRoot,
    "docs",
    "knowledge-base",
    "generated",
    "graphify",
    "graph.html",
  );
  const guidePath = await findGuidePath(repoRoot);
  const standardsPath = findStandardsPath(repoRoot);

  return {
    repoRoot,
    available: true,
    reportPath,
    guidePath,
    standardsPath,
    graphJsonPath,
    graphHtmlPath,
    reportRel: toRepoRelative(repoRoot, reportPath),
    guideRel: toRepoRelative(repoRoot, guidePath),
    standardsRel: toRepoRelative(repoRoot, standardsPath),
    graphJsonRel: toRepoRelative(repoRoot, graphJsonPath),
    graphHtmlRel: toRepoRelative(repoRoot, graphHtmlPath),
  };
}

async function getSessionState(sessionId, cwd) {
  const existing = sessionStates.get(sessionId);
  if (existing && existing.cwd === cwd) {
    return existing;
  }

  const context = await resolveGraphifyContext(cwd);
  const next = {
    cwd,
    context,
    reportRead: false,
    guideRead: false,
    standardsRead: false,
    activationLogged: false,
  };
  sessionStates.set(sessionId, next);
  return next;
}

function buildAdditionalContext(state) {
  const { context, reportRead, guideRead, standardsRead } = state;
  if (!context.available) {
    return undefined;
  }

  const lines = [
    "graphify-guard: A local graphify knowledge base exists for this repository.",
    `Read ${context.reportRel} before answering repository architecture/code questions or before using raw repo search tools.`,
  ];

  if (context.guideRel) {
    lines.push(
      `Also use ${context.guideRel} as the project-facing summary of crate choices, architecture rules, and implementation order.`,
    );
  }

  if (!reportRead) {
    lines.push(
      "Until the graphify report has been read in this session, prefer reading the report first instead of grepping or shell-searching the repository.",
    );
  } else {
    lines.push(
      "The graphify report has already been read in this session; continue to treat it as the default orientation source for repo-specific reasoning.",
    );
  }

  if (context.standardsRel) {
    lines.push(
      `Use ${context.standardsRel} as the repository coding rulebook for architecture, workflow priority, state ownership, and review decisions.`,
    );
  }

  if (context.graphJsonRel) {
    lines.push(
      `For deeper graph inspection, raw graph data is available at ${context.graphJsonRel}. Do not paste the entire JSON into context; use the report for orientation first.`,
    );
  }

  if (context.graphHtmlRel) {
    lines.push(
      `An interactive graph snapshot also exists at ${context.graphHtmlRel} for local inspection outside the chat.`,
    );
  }

  if (!guideRead && context.guideRel) {
    lines.push(
      "If the request is about product direction, crate selection, session/workspace/git state, or implementation sequencing, read the local guide as well.",
    );
  }

  if (!standardsRead && context.standardsRel) {
    lines.push(
      "Before changing code, read the coding standards document. It is mandatory for implementation work, not optional background reading.",
    );
  } else if (standardsRead) {
    lines.push(
      "The coding standards document has already been read in this session; continue to apply it as the default implementation rulebook.",
    );
  }

  return lines.join(" ");
}

function isRequiredKnowledgePath(state, candidatePath) {
  const resolved = normalizePath(candidatePath);
  if (!resolved || !state.context.available) {
    return false;
  }

  return (
    resolved === normalizePath(state.context.reportPath) ||
    resolved === normalizePath(state.context.guidePath) ||
    resolved === normalizePath(state.context.standardsPath)
  );
}

function isReportPath(state, candidatePath) {
  return (
    normalizePath(candidatePath) === normalizePath(state.context.reportPath)
  );
}

function isGuidePath(state, candidatePath) {
  return normalizePath(candidatePath) === normalizePath(state.context.guidePath);
}

function isStandardsPath(state, candidatePath) {
  return (
    normalizePath(candidatePath) === normalizePath(state.context.standardsPath)
  );
}

const session = await joinSession({
  hooks: {
    onSessionStart: async (input, invocation) => {
      const state = await getSessionState(invocation.sessionId, input.cwd);
      if (!state.context.available) {
        return;
      }

      if (!state.activationLogged) {
        state.activationLogged = true;
        await session.log(
          `graphify-guard active: read ${state.context.reportRel} before raw repo search and ${state.context.standardsRel ?? "the coding standards"} before code changes.`,
          { ephemeral: true },
        );
      }

      return {
        additionalContext: buildAdditionalContext(state),
      };
    },

    onUserPromptSubmitted: async (input, invocation) => {
      const state = await getSessionState(invocation.sessionId, input.cwd);
      if (!state.context.available) {
        return;
      }

      return {
        additionalContext: buildAdditionalContext(state),
      };
    },

    onPreToolUse: async (input, invocation) => {
      const state = await getSessionState(invocation.sessionId, input.cwd);
      if (!state.context.available) {
        return;
      }

      if (input.toolName === "view") {
        return {
          additionalContext: buildAdditionalContext(state),
        };
      }

      // Allow safe bash commands (git, builds, process mgmt) without guard
      if (input.toolName === "bash" && input.toolArgs?.command) {
        const cmd = input.toolArgs.command.trimStart();
        const isSafe = SAFE_BASH_PREFIXES.some((prefix) => cmd.startsWith(prefix));
        if (isSafe) {
          return {
            additionalContext: buildAdditionalContext(state),
          };
        }
      }

      const needsKnowledgeRead = BLOCKED_TOOLS.has(input.toolName) && !state.reportRead;
      const needsStandardsRead =
        (input.toolName === "apply_patch" ||
          input.toolName === "task" ||
          input.toolName === "bash") &&
        !state.standardsRead;

      if (needsKnowledgeRead) {
        return {
          permissionDecision: "deny",
          permissionDecisionReason: `Read graphify knowledge first: ${state.context.reportRel}${state.context.guideRel ? ` (and ${state.context.guideRel} for project guidance)` : ""}.`,
          additionalContext: buildAdditionalContext(state),
        };
      }

      if (needsStandardsRead) {
        return {
          permissionDecision: "deny",
          permissionDecisionReason: `Read the coding rulebook first: ${state.context.standardsRel}.`,
          additionalContext: buildAdditionalContext(state),
        };
      }
    },

    onPostToolUse: async (input, invocation) => {
      const state = await getSessionState(invocation.sessionId, input.cwd);
      if (!state.context.available || input.toolName !== "view") {
        return;
      }

      const viewedPath = input.toolArgs?.path;
      if (!isRequiredKnowledgePath(state, viewedPath)) {
        return;
      }

      if (isReportPath(state, viewedPath)) {
        state.reportRead = true;
      }
      if (isGuidePath(state, viewedPath)) {
        state.guideRead = true;
      }
      if (isStandardsPath(state, viewedPath)) {
        state.standardsRead = true;
      }

      if (state.reportRead) {
        await session.log("graphify-guard: graphify report read for this session.", {
          ephemeral: true,
        });
      }
      if (state.standardsRead) {
        await session.log(
          "graphify-guard: coding standards read for this session.",
          {
            ephemeral: true,
          },
        );
      }
    },
  },
});

session.on("session.shutdown", (event) => {
  if (event?.sessionId) {
    sessionStates.delete(event.sessionId);
  }
});
