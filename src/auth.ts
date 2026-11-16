import { clearRadio } from "./radio";
import { account as storedAccount, clear, signIn as openPage, stop } from "./session";
import type { Account } from "./types";

/** Opens the local sign-in page and waits until a server is in use. */
export async function signIn(): Promise<Account> {
  return openPage();
}

export function signOut(): void {
  stop();
  clear();
  clearRadio();
}

export function account(): Account | null {
  return storedAccount();
}
