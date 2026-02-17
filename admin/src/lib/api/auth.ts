import { apiFetch } from "./client";

export async function login(signedEvent: object): Promise<void> {
  await apiFetch("/login", {
    method: "POST",
    body: JSON.stringify(signedEvent),
  });
}

export async function logout(): Promise<void> {
  await apiFetch("/logout", { method: "POST" });
}
