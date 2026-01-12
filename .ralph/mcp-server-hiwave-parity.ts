#!/usr/bin/env node
/**
 * MCP Server for HiWave Parity Testing
 * Allows Claude to run tests, read results, and modify RustKit code
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { exec } from 'child_process';
import { promisify } from 'util';
import { readFile, writeFile, readdir } from 'fs/promises';

const execAsync = promisify(exec);

// Configuration
const HIWAVE_ROOT = '/Users/petecopeland/Repos/hiwave/hiwave-macos';

// Create server
const server = new Server(
  {
    name: 'hiwave-parity-tester',
    version: '1.0.0',
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// Tool implementations
async function runParityTest(args: { test?: string; scope?: string }) {
  const { test, scope = 'all' } = args;
  
  const cmd = test
    ? `cd ${HIWAVE_ROOT} && python3 scripts/parity_test.py --test ${test}`
    : `cd ${HIWAVE_ROOT} && python3 scripts/parity_test.py --scope ${scope}`;

  try {
    const { stdout, stderr } = await execAsync(cmd, {
      timeout: 600000, // 10 minutes
      maxBuffer: 10 * 1024 * 1024, // 10MB
    });

    return {
      success: true,
      output: stdout + stderr,
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
      output: error.stdout + error.stderr,
    };
  }
}

async function getParityStatus() {
  try {
    const resultsPath = `${HIWAVE_ROOT}/parity-baseline/parity_test_results.json`;
    const content = await readFile(resultsPath, 'utf-8');
    const results = JSON.parse(content);

    // Extract and sort tests by worst diff
    const tests = Object.entries(results.tests || {})
      .map(([name, data]: [string, any]) => ({
        name,
        diff: data.diff_percentage,
        passed: data.passed,
        threshold: data.threshold,
      }))
      .sort((a, b) => b.diff - a.diff);

    return {
      success: true,
      timestamp: results.timestamp,
      summary: results.summary,
      tests,
      worst_cases: tests.slice(0, 5),
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
    };
  }
}

async function buildRustKit() {
  try {
    const { stdout, stderr } = await execAsync(
      `cd ${HIWAVE_ROOT} && cargo build --release`,
      {
        timeout: 600000, // 10 minutes
      }
    );

    return {
      success: true,
      output: stdout + stderr,
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
      output: error.stdout + error.stderr,
    };
  }
}

async function listRustKitFiles(args: { crate?: string }) {
  try {
    const cratePath = args.crate
      ? `${HIWAVE_ROOT}/crates/${args.crate}`
      : `${HIWAVE_ROOT}/crates`;

    const { stdout } = await execAsync(`find ${cratePath} -name "*.rs" -type f`);
    
    return {
      success: true,
      files: stdout.trim().split('\n').map(f => f.replace(HIWAVE_ROOT + '/', '')),
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
    };
  }
}

async function readRustKitFile(args: { path: string }) {
  try {
    const fullPath = `${HIWAVE_ROOT}/${args.path}`;
    const content = await readFile(fullPath, 'utf-8');
    
    return {
      success: true,
      path: args.path,
      content,
      lines: content.split('\n').length,
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
    };
  }
}

async function writeRustKitFile(args: { path: string; content: string }) {
  try {
    const fullPath = `${HIWAVE_ROOT}/${args.path}`;
    await writeFile(fullPath, args.content, 'utf-8');
    
    return {
      success: true,
      path: args.path,
      bytes_written: Buffer.byteLength(args.content, 'utf-8'),
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
    };
  }
}

async function runCargoTest() {
  try {
    const { stdout, stderr } = await execAsync(
      `cd ${HIWAVE_ROOT} && cargo test`,
      { timeout: 300000 }
    );

    return {
      success: true,
      output: stdout + stderr,
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
      output: error.stdout + error.stderr,
    };
  }
}

async function formatRustCode(args: { path?: string }) {
  try {
    const cmd = args.path
      ? `cd ${HIWAVE_ROOT} && cargo fmt -- ${args.path}`
      : `cd ${HIWAVE_ROOT} && cargo fmt`;

    const { stdout, stderr } = await execAsync(cmd);

    return {
      success: true,
      output: stdout + stderr,
    };
  } catch (error: any) {
    return {
      success: false,
      error: error.message,
    };
  }
}

// Register tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: 'run_parity_test',
        description: 'Run HiWave pixel parity tests comparing RustKit vs Chrome rendering',
        inputSchema: {
          type: 'object',
          properties: {
            test: {
              type: 'string',
              description: 'Specific test to run (e.g., "gradient-backgrounds")',
            },
            scope: {
              type: 'string',
              description: 'Test scope: "all", "failing", or "passing"',
              enum: ['all', 'failing', 'passing'],
            },
          },
        },
      },
      {
        name: 'get_parity_status',
        description: 'Get current parity test results, ranked by worst performance',
        inputSchema: {
          type: 'object',
          properties: {},
        },
      },
      {
        name: 'build_rustkit',
        description: 'Build RustKit with cargo build --release',
        inputSchema: {
          type: 'object',
          properties: {},
        },
      },
      {
        name: 'list_rustkit_files',
        description: 'List Rust source files in RustKit crates',
        inputSchema: {
          type: 'object',
          properties: {
            crate: {
              type: 'string',
              description: 'Specific crate to list (e.g., "rustkit-css", "rustkit-layout")',
            },
          },
        },
      },
      {
        name: 'read_rustkit_file',
        description: 'Read a RustKit source file',
        inputSchema: {
          type: 'object',
          properties: {
            path: {
              type: 'string',
              description: 'File path relative to hiwave-macos/ (e.g., "crates/rustkit-css/src/gradient.rs")',
            },
          },
          required: ['path'],
        },
      },
      {
        name: 'write_rustkit_file',
        description: 'Write to a RustKit source file',
        inputSchema: {
          type: 'object',
          properties: {
            path: {
              type: 'string',
              description: 'File path relative to hiwave-macos/',
            },
            content: {
              type: 'string',
              description: 'Complete file content to write',
            },
          },
          required: ['path', 'content'],
        },
      },
      {
        name: 'run_cargo_test',
        description: 'Run Rust unit tests with cargo test',
        inputSchema: {
          type: 'object',
          properties: {},
        },
      },
      {
        name: 'format_rust_code',
        description: 'Format Rust code with cargo fmt',
        inputSchema: {
          type: 'object',
          properties: {
            path: {
              type: 'string',
              description: 'Optional: specific file to format',
            },
          },
        },
      },
    ],
  };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  let result;
  switch (name) {
    case 'run_parity_test':
      result = await runParityTest(args || {});
      break;
    case 'get_parity_status':
      result = await getParityStatus();
      break;
    case 'build_rustkit':
      result = await buildRustKit();
      break;
    case 'list_rustkit_files':
      result = await listRustKitFiles(args || {});
      break;
    case 'read_rustkit_file':
      result = await readRustKitFile(args as any);
      break;
    case 'write_rustkit_file':
      result = await writeRustKitFile(args as any);
      break;
    case 'run_cargo_test':
      result = await runCargoTest();
      break;
    case 'format_rust_code':
      result = await formatRustCode(args || {});
      break;
    default:
      throw new Error(`Unknown tool: ${name}`);
  }

  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify(result, null, 2),
      },
    ],
  };
});

// Start server
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error('HiWave Parity Testing MCP Server started');
}

main().catch(console.error);