import { execSync } from "child_process";
import { homedir } from "os";
import { join } from "path";
import { PLIST_NAME } from "./config";

const PLIST_PATH = join(homedir(), "Library", "LaunchAgents", `${PLIST_NAME}.plist`);

function getUid(): string {
  return execSync("id -u", { encoding: "utf-8" }).trim();
}

export function restartService(): void {
  const uid = getUid();
  // Stop (ignore errors if not running)
  try {
    execSync(`launchctl bootout gui/${uid}/${PLIST_NAME}`, { encoding: "utf-8" });
  } catch {
    // Service may not be running
  }
  // Start
  execSync(`launchctl bootstrap gui/${uid} "${PLIST_PATH}"`, { encoding: "utf-8" });
}

export function isServiceRunning(): boolean {
  const uid = getUid();
  try {
    const output = execSync(`launchctl print gui/${uid}/${PLIST_NAME}`, {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    // Check for "state = running" or active PID
    return output.includes("state = running") || output.includes("pid = ");
  } catch {
    return false;
  }
}
