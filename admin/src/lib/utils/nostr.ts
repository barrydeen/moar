declare global {
  interface Window {
    nostr?: {
      getPublicKey(): Promise<string>;
      signEvent(event: {
        kind: number;
        created_at: number;
        tags: string[][];
        content: string;
      }): Promise<{
        id: string;
        pubkey: string;
        created_at: number;
        kind: number;
        tags: string[][];
        content: string;
        sig: string;
      }>;
    };
  }
}

export function hasNostrExtension(): boolean {
  return typeof window !== "undefined" && !!window.nostr;
}

export async function createLoginEvent(): Promise<object> {
  if (!window.nostr) {
    throw new Error("No Nostr extension found. Install nos2x, Alby, or another NIP-07 extension.");
  }

  const pubkey = await window.nostr.getPublicKey();
  const now = Math.floor(Date.now() / 1000);

  const event = await window.nostr.signEvent({
    kind: 27235,
    created_at: now,
    tags: [
      ["u", `${window.location.origin}/api/login`],
      ["method", "POST"],
    ],
    content: "",
  });

  return event;
}
