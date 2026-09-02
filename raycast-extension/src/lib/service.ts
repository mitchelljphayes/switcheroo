import { execFileSync } from "child_process";
import { homedir } from "os";
import { existsSync, statSync } from "fs";
import { join } from "path";
import { PLIST_NAME } from "./config";

const LAUNCHCTL = "/bin/launchctl";
const ID = "/usr/bin/id";
const PLUTIL = "/usr/bin/plutil";

const PLIST_PATH = join(homedir(), "Library", "LaunchAgents", `${PLIST_NAME}.plist`);
const EXPECTED_EXEC_SUFFIX = ".local/bin/Switcheroo.app/Contents/MacOS/switcheroo";
const EXPECTED_EXEC = join(homedir(), EXPECTED_EXEC_SUFFIX);

function getUid(): string {
  return execFileSync(ID, ["-u"], { encoding: "utf-8" }).trim();
}

/** Parse the `program =` line from `launchctl print` output. Returns the
 * program path or empty string if not found. Never dumps environment. */
function parseLaunchctlProgram(output: string): string {
  for (const line of output.split("\n")) {
    const m = line.match(/^\s*program\s*=\s*(.*)$/);
    if (m) return m[1];
  }
  return "";
}

/** True if the loaded job's program matches the expected Switcheroo executable. */
function loadedJobIsSwitcheroo(uid: string): boolean {
  try {
    const output = execFileSync(LAUNCHCTL, ["print", `gui/${uid}/${PLIST_NAME}`], {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    const prog = parseLaunchctlProgram(output);
    return prog === EXPECTED_EXEC;
  } catch {
    return false;
  }
}

/** True if the plist file exists and its ProgramArguments[0] matches the
 * expected Switcheroo executable. Validates via plutil. */
function plistIsSwitcheroo(): boolean {
  if (!existsSync(PLIST_PATH)) return false;
  // Verify ownership: plist must be owned by the current user.
  try {
    const stat = statSync(PLIST_PATH);
    if (stat.uid !== Number(getUid())) return false;
  } catch {
    return false;
  }
  try {
    const prog = execFileSync(
      PLUTIL,
      ["-extract", "ProgramArguments.0", "raw", "-o", "-", PLIST_PATH],
      { encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();
    return prog === EXPECTED_EXEC;
  } catch {
    return false;
  }
}

export function restartService(): void {
  const uid = getUid();

  // Stop: only bootout if the loaded job is verified Switcheroo. Refuse
  // to stop a foreign/ambiguous job sharing the label (collision safety).
  try {
    // Check if the label is loaded at all.
    execFileSync(LAUNCHCTL, ["print", `gui/${uid}/${PLIST_NAME}`], {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    // Label is loaded — verify it's ours before bootout.
    if (loadedJobIsSwitcheroo(uid)) {
      execFileSync(LAUNCHCTL, ["bootout", `gui/${uid}/${PLIST_NAME}`], {
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      });
    } else {
      // Loaded but not Switcheroo — refuse to stop a foreign job.
      throw new Error(
        `Refusing to bootout ${PLIST_NAME}: loaded job program does not match Switcheroo (possible collision)`,
      );
    }
  } catch (e) {
    // Not loaded — that's fine, proceed to bootstrap.
    if (e instanceof Error && e.message.startsWith("Refusing")) throw e;
  }

  // Start: validate the plist is Switcheroo before bootstrap.
  if (!plistIsSwitcheroo()) {
    throw new Error(
      `Refusing to bootstrap ${PLIST_NAME}: plist does not match Switcheroo or is missing`,
    );
  }
  execFileSync(LAUNCHCTL, ["bootstrap", `gui/${uid}`, PLIST_PATH], {
    encoding: "utf-8",
    stdio: ["pipe", "pipe", "pipe"],
  });
}

export function isServiceRunning(): boolean {
  const uid = getUid();
  try {
    const output = execFileSync(
      LAUNCHCTL,
      ["print", `gui/${uid}/${PLIST_NAME}`],
      {
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
    return output.includes("state = running") || output.includes("pid = ");
  } catch {
    return false;
  }
}
