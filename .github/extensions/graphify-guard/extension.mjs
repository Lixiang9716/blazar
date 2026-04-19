import { access, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { joinSession } from "@github/copilot-sdk/extension";

const sessionStates = new Map();

const BLOCKED_TOOLS = new Set([
  "bash",
  "glob",
  "rg",
  "task",
  "github-mcp-server-search_code",
  "github-mcp-server-get_file_contents",
]);

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

  return {
    repoRoot,
    available: true,
    reportPath,
    guidePath,
    graphJsonPath,
    graphHtmlPath,
    reportRel: toRepoRelative(repoRoot, reportPath),
    guideRel: toRepoRelative(repoRoot, guidePath),
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
    activationLogged: false,
  };
  sessionStates.set(sessionId, next);
  return next;
}

function buildAdditionalContext(state) {
  const { context, reportRead, guideRead } = state;
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

  return lines.join(" ");
}

function isRequiredKnowledgePath(state, candidatePath) {
  const resolved = normalizePath(candidatePath);
  if (!resolved || !state.context.available) {
    return false;
  }

  return (
    resolved === normalizePath(state.context.reportPath) ||
    resolved === normalizePath(state.context.guidePath)
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
          `graphify-guard active: read ${state.context.reportRel} before raw repo search.`,
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
      if (!state.context.available || state.reportRead) {
        return;
      }

      if (input.toolName === "view") {
        return {
          additionalContext: buildAdditionalContext(state),
        };
      }

      if (BLOCKED_TOOLS.has(input.toolName)) {
        return {
          permissionDecision: "deny",
          permissionDecisionReason: `Read graphify knowledge first: ${state.context.reportRel}${state.context.guideRel ? ` (and ${state.context.guideRel} for project guidance)` : ""}.`,
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

      if (state.reportRead) {
        await session.log("graphify-guard: graphify report read for this session.", {
          ephemeral: true,
        });
      }
    },
  },
});

session.on("session.shutdown", (event) => {
  if (event?.sessionId) {
    sessionStates.delete(event.sessionId);
  }
});
