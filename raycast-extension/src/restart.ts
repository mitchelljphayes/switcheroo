import { showToast, Toast } from "@raycast/api";
import { restartService } from "./lib/service";

export default async function Command() {
  try {
    restartService();
    await showToast({ style: Toast.Style.Success, title: "Rebind restarted" });
  } catch (e) {
    await showToast({
      style: Toast.Style.Failure,
      title: "Failed to restart Rebind",
      message: String(e),
    });
  }
}
