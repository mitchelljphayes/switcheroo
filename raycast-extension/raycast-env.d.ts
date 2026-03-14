/// <reference types="@raycast/api">

/* 🚧 🚧 🚧
 * This file is auto-generated from the extension's manifest.
 * Do not modify manually. Instead, update the `package.json` file.
 * 🚧 🚧 🚧 */

/* eslint-disable @typescript-eslint/ban-types */

type ExtensionPreferences = {}

/** Preferences accessible in all the extension's commands */
declare type Preferences = ExtensionPreferences

declare namespace Preferences {
  /** Preferences accessible in the `view-remaps` command */
  export type ViewRemaps = ExtensionPreferences & {}
  /** Preferences accessible in the `add-remap` command */
  export type AddRemap = ExtensionPreferences & {}
  /** Preferences accessible in the `restart` command */
  export type Restart = ExtensionPreferences & {}
  /** Preferences accessible in the `view-logs` command */
  export type ViewLogs = ExtensionPreferences & {}
  /** Preferences accessible in the `edit-config` command */
  export type EditConfig = ExtensionPreferences & {}
}

declare namespace Arguments {
  /** Arguments passed to the `view-remaps` command */
  export type ViewRemaps = {}
  /** Arguments passed to the `add-remap` command */
  export type AddRemap = {}
  /** Arguments passed to the `restart` command */
  export type Restart = {}
  /** Arguments passed to the `view-logs` command */
  export type ViewLogs = {}
  /** Arguments passed to the `edit-config` command */
  export type EditConfig = {}
}

