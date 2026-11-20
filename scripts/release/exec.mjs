// Child-process helpers: run a command, stream it to a log, keep a short tail
// so a failure can say what went wrong without dumping the whole log.
import { spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';

const TAIL_BYTES = 6000;

export class CommandError extends Error {
  constructor(message, { code, tail }) {
    super(message);
    this.name = 'CommandError';
    this.code = code;
    this.tail = tail;
  }
}

function keepTail(previous, chunk) {
  const next = previous + chunk;
  return next.length > TAIL_BYTES ? next.slice(-TAIL_BYTES) : next;
}

/**
 * Run a command without a shell.
 * - capture: return stdout as a string instead of printing it
 * - logFile: append stdout/stderr to this file instead of the terminal
 * - allowFail: resolve with the exit code instead of throwing
 */
export function run(cmd, args, options = {}) {
  const { cwd, env, logFile, capture = false, allowFail = false, input, quiet = false } = options;
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { cwd, env: env ?? process.env, stdio: ['pipe', 'pipe', 'pipe'] });
    const log = logFile ? createWriteStream(logFile, { flags: 'a' }) : null;
    let stdout = '';
    let tail = '';
    const onData = (stream) => (chunk) => {
      const text = chunk.toString();
      tail = keepTail(tail, text);
      if (capture && stream === 'out') stdout += text;
      if (log) log.write(text);
      else if (!capture && !quiet) (stream === 'out' ? process.stdout : process.stderr).write(text);
    };
    child.stdout.on('data', onData('out'));
    child.stderr.on('data', onData('err'));
    child.on('error', (error) => {
      log?.end();
      reject(new CommandError(`${cmd} could not start: ${error.message}`, { code: -1, tail }));
    });
    child.on('close', (code) => {
      log?.end();
      if (code === 0 || allowFail) {
        resolve({ code, stdout, tail });
        return;
      }
      reject(new CommandError(`${cmd} ${args.slice(0, 3).join(' ')} … exited with ${code}`, { code, tail }));
    });
    child.stdin.end(input ?? '');
  });
}

export async function capture(cmd, args, options = {}) {
  const { stdout } = await run(cmd, args, { ...options, capture: true });
  return stdout.trim();
}
