import { spawn } from "node:child_process";

export class TreeWorkMcpClient {
  constructor(pluginRoot, buildDir, clientVersion = "unknown") {
    this.pluginRoot = pluginRoot;
    this.buildDir = buildDir;
    this.clientVersion = clientVersion;
    this.process = undefined;
    this.queue = [];
    this.waiters = [];
    this.stdoutBuffer = "";
    this.stderrTail = "";
    this.nextId = 1;
    this.initialized = false;
    this.serial = Promise.resolve();
  }

  rejectWaiters(error) {
    for (const waiter of this.waiters.splice(0)) waiter.reject(error);
  }

  startProcess() {
    if (this.process && this.process.exitCode === null) return;
    this.queue = [];
    this.stdoutBuffer = "";
    this.stderrTail = "";
    const child = spawn("bash", ["./scripts/start-mcp.sh"], {
      cwd: this.pluginRoot,
      env: {
        ...process.env,
        TREEWORK_PLUGIN_ROOT: this.pluginRoot,
        TREEWORK_BUILD_DIR: this.buildDir,
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.process = child;
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      this.stdoutBuffer += chunk;
      let newline = this.stdoutBuffer.indexOf("\n");
      while (newline >= 0) {
        const rawLine = this.stdoutBuffer.slice(0, newline);
        this.stdoutBuffer = this.stdoutBuffer.slice(newline + 1);
        const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;
        const waiter = this.waiters.shift();
        if (waiter) waiter.resolve(line);
        else this.queue.push(line);
        newline = this.stdoutBuffer.indexOf("\n");
      }
    });
    child.stderr.on("data", (chunk) => {
      this.stderrTail = `${this.stderrTail}${String(chunk)}`.slice(-12000);
    });
    child.on("error", (cause) => {
      const error = new Error(`TreeWork MCP server failed to start: ${cause.message}`, { cause });
      this.rejectWaiters(error);
      this.initialized = false;
    });
    child.on("exit", (code, signal) => {
      const error = new Error(
        `TreeWork MCP server exited (${signal ?? code ?? "unknown"})${
          this.stderrTail ? `: ${this.stderrTail.trim()}` : ""
        }`,
      );
      this.rejectWaiters(error);
      this.initialized = false;
    });
  }

  nextLine(timeoutMs, signal) {
    if (signal?.aborted) return Promise.reject(new Error("TreeWork MCP request cancelled"));
    const queued = this.queue.shift();
    if (queued !== undefined) return Promise.resolve(queued);

    return new Promise((resolve, reject) => {
      let settled = false;
      let timer;
      const finish = (callback) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        signal?.removeEventListener("abort", onAbort);
        const index = this.waiters.indexOf(waiter);
        if (index >= 0) this.waiters.splice(index, 1);
        callback();
      };
      const waiter = {
        resolve: (line) => finish(() => resolve(line)),
        reject: (error) => finish(() => reject(error)),
      };
      const onAbort = () => {
        finish(() => reject(new Error("TreeWork MCP request cancelled")));
        this.close(new Error("TreeWork MCP request cancelled"));
      };
      timer = setTimeout(() => {
        const error = new Error(`TreeWork MCP request timed out after ${timeoutMs}ms`);
        finish(() => reject(error));
        this.close(error);
      }, timeoutMs);
      signal?.addEventListener("abort", onAbort, { once: true });
      this.waiters.push(waiter);
    });
  }

  async rawRequest(method, params, timeoutMs, signal) {
    if (signal?.aborted) throw new Error("TreeWork MCP request cancelled");
    const child = this.process;
    if (!child || child.exitCode !== null) throw new Error("TreeWork MCP server is not running");
    const id = this.nextId++;
    const payload = { jsonrpc: "2.0", id, method, ...(params ? { params } : {}) };
    child.stdin.write(`${JSON.stringify(payload)}\n`);
    const deadline = Date.now() + timeoutMs;

    while (true) {
      const remaining = deadline - Date.now();
      if (remaining <= 0) throw new Error(`TreeWork MCP request timed out after ${timeoutMs}ms`);
      const line = await this.nextLine(remaining, signal);
      let response;
      try {
        response = JSON.parse(line);
      } catch (error) {
        throw new Error(`TreeWork MCP returned invalid JSON: ${String(error)}`);
      }
      if (response.id === undefined) continue;
      if (response.id !== id) {
        throw new Error(`TreeWork MCP response id mismatch: expected ${id}, received ${response.id}`);
      }
      if (response.error) {
        throw new Error(
          `TreeWork MCP error ${response.error.code ?? ""}: ${response.error.message ?? "unknown error"}`.trim(),
        );
      }
      return response.result;
    }
  }

  notify(method) {
    const child = this.process;
    if (!child || child.exitCode !== null) throw new Error("TreeWork MCP server is not running");
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method })}\n`);
  }

  async ensureInitialized(signal) {
    if (signal?.aborted) throw new Error("TreeWork MCP request cancelled");
    if (this.initialized && this.process?.exitCode === null) return;
    this.startProcess();
    await this.rawRequest(
      "initialize",
      {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "treework-pi-adapter", version: this.clientVersion },
      },
      60_000,
      signal,
    );
    this.notify("notifications/initialized");
    this.initialized = true;
  }

  async callTool(name, args, signal) {
    const execute = async () => {
      if (signal?.aborted) throw new Error("TreeWork MCP request cancelled");
      await this.ensureInitialized(signal);
      const result = await this.rawRequest(
        "tools/call",
        { name, arguments: args },
        name === "treework_project_map" ? 330_000 : 60_000,
        signal,
      );
      if (!result || !Array.isArray(result.content)) {
        throw new Error(`TreeWork MCP tool ${name} returned an invalid result`);
      }
      if (result.isError) {
        const text = result.content.map((part) => part.text).join("\n");
        throw new Error(text || `TreeWork MCP tool ${name} failed`);
      }
      return result;
    };

    const pending = this.serial.then(execute, execute);
    this.serial = pending.then(
      () => undefined,
      () => undefined,
    );
    return pending;
  }

  close(reason = new Error("TreeWork MCP client closed")) {
    const child = this.process;
    this.process = undefined;
    this.initialized = false;
    this.queue = [];
    this.stdoutBuffer = "";
    this.rejectWaiters(reason);
    if (!child || child.exitCode !== null) return;
    child.stdin.end();
    setTimeout(() => {
      if (child.exitCode === null) child.kill("SIGTERM");
    }, 1000).unref();
  }
}
