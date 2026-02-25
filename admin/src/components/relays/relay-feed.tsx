"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { deleteEvent, fetchOgTags } from "@/lib/api/relays";
import { fetchProfiles } from "@/lib/nostr/pool";
import { useDiscoveryRelays } from "@/lib/hooks/use-wot";
import { useRelay } from "@/lib/hooks/use-relays";
import { useStatus } from "@/lib/hooks/use-status";
import { toast } from "sonner";
import { Trash2, Wifi, WifiOff } from "lucide-react";
import type { NostrProfile } from "@/lib/types/nostr";

interface FeedEvent {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  content: string;
  tags: string[][];
}

interface OgData {
  title?: string;
  description?: string;
  image?: string;
}

interface RelayFeedProps {
  relayId: string;
}

const MAX_EVENTS = 200;

const IMAGE_EXTENSIONS = /\.(jpg|jpeg|png|gif|webp|svg)(\?.*)?$/i;
const VIDEO_EXTENSIONS = /\.(mp4|webm|mov)(\?.*)?$/i;
const URL_REGEX = /https?:\/\/[^\s<>"]+/g;

function timeAgo(timestamp: number): string {
  const seconds = Math.floor(Date.now() / 1000 - timestamp);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function truncatePubkey(hex: string): string {
  if (hex.length < 16) return hex;
  return `${hex.slice(0, 8)}...${hex.slice(-8)}`;
}

function NoteContent({
  content,
  ogCache,
  onFetchOg,
}: {
  content: string;
  ogCache: Map<string, OgData | null>;
  onFetchOg: (url: string) => void;
}) {
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  const matches = [...content.matchAll(URL_REGEX)];

  for (const match of matches) {
    const url = match[0];
    const index = match.index!;

    if (index > lastIndex) {
      parts.push(content.slice(lastIndex, index));
    }

    if (IMAGE_EXTENSIONS.test(url)) {
      parts.push(
        <img
          key={index}
          src={url}
          alt=""
          className="mt-2 max-w-full max-h-80 rounded-md"
          loading="lazy"
        />
      );
    } else if (VIDEO_EXTENSIONS.test(url)) {
      parts.push(
        <video
          key={index}
          src={url}
          controls
          className="mt-2 max-w-full max-h-80 rounded-md"
          preload="metadata"
        />
      );
    } else {
      const og = ogCache.get(url);
      if (og === undefined) {
        onFetchOg(url);
        parts.push(
          <a
            key={index}
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-400 hover:underline break-all"
          >
            {url}
          </a>
        );
      } else if (og && (og.title || og.description || og.image)) {
        parts.push(
          <a
            key={index}
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            className="mt-2 block rounded-md border border-border overflow-hidden hover:border-primary/50 transition-colors"
          >
            {og.image && (
              <img
                src={og.image}
                alt=""
                className="w-full max-h-48 object-cover"
                loading="lazy"
              />
            )}
            <div className="p-3">
              {og.title && (
                <div className="font-medium text-sm text-foreground line-clamp-1">
                  {og.title}
                </div>
              )}
              {og.description && (
                <div className="text-xs text-muted-foreground mt-1 line-clamp-2">
                  {og.description}
                </div>
              )}
              <div className="text-xs text-muted-foreground/60 mt-1 truncate">
                {new URL(url).hostname}
              </div>
            </div>
          </a>
        );
      } else {
        parts.push(
          <a
            key={index}
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-400 hover:underline break-all"
          >
            {url}
          </a>
        );
      }
    }

    lastIndex = index + url.length;
  }

  if (lastIndex < content.length) {
    parts.push(content.slice(lastIndex));
  }

  return <div className="whitespace-pre-wrap break-words">{parts}</div>;
}

function EventCard({
  event,
  profile,
  relayId,
  ogCache,
  onFetchOg,
  onDeleted,
}: {
  event: FeedEvent;
  profile?: NostrProfile;
  relayId: string;
  ogCache: Map<string, OgData | null>;
  onFetchOg: (url: string) => void;
  onDeleted: (id: string) => void;
}) {
  const [deleting, setDeleting] = useState(false);

  async function handleDelete() {
    if (!confirm("Delete this event from the relay?")) return;
    setDeleting(true);
    try {
      await deleteEvent(relayId, event.id);
      onDeleted(event.id);
      toast.success("Event deleted");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Delete failed");
    } finally {
      setDeleting(false);
    }
  }

  const displayName =
    profile?.display_name || profile?.name || truncatePubkey(event.pubkey);

  return (
    <div className="rounded-lg border border-border bg-card p-4 space-y-2">
      <div className="flex items-center gap-3">
        {profile?.picture ? (
          <img
            src={profile.picture}
            alt=""
            className="w-10 h-10 rounded-full object-cover flex-shrink-0"
          />
        ) : (
          <div className="w-10 h-10 rounded-full bg-muted flex items-center justify-center flex-shrink-0">
            <span className="text-muted-foreground text-xs">
              {displayName.slice(0, 2).toUpperCase()}
            </span>
          </div>
        )}
        <div className="flex-1 min-w-0">
          <div className="font-medium text-sm truncate">{displayName}</div>
          <div className="text-xs text-muted-foreground">
            {timeAgo(event.created_at)} &middot; kind {event.kind}
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 text-muted-foreground hover:text-destructive flex-shrink-0"
          onClick={handleDelete}
          disabled={deleting}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      </div>
      <div className="text-sm">
        <NoteContent content={event.content} ogCache={ogCache} onFetchOg={onFetchOg} />
      </div>
    </div>
  );
}

export function RelayFeed({ relayId }: RelayFeedProps) {
  const [events, setEvents] = useState<FeedEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const [profiles, setProfiles] = useState<Map<string, NostrProfile>>(new Map());
  const [ogCache, setOgCache] = useState<Map<string, OgData | null>>(new Map());
  const { data: relay } = useRelay(relayId);
  const { data: status } = useStatus();
  const { data: discoveryRelays } = useDiscoveryRelays();
  const containerRef = useRef<HTMLDivElement>(null);
  const autoScrollRef = useRef(true);
  const wsRef = useRef<WebSocket | null>(null);
  const profileQueueRef = useRef<Set<string>>(new Set());
  const profileTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const profilesRef = useRef<Map<string, NostrProfile>>(profiles);
  profilesRef.current = profiles;
  const discoveryRelaysRef = useRef(discoveryRelays);
  discoveryRelaysRef.current = discoveryRelays;
  const ogPendingRef = useRef<Set<string>>(new Set());
  const gotEoseRef = useRef(false);

  const queueProfileFetch = useCallback((pubkeys: string[]) => {
    const newPubkeys = pubkeys.filter(
      (pk) => !profilesRef.current.has(pk) && !profileQueueRef.current.has(pk)
    );
    if (newPubkeys.length === 0) return;

    for (const pk of newPubkeys) {
      profileQueueRef.current.add(pk);
    }

    if (profileTimerRef.current) clearTimeout(profileTimerRef.current);
    profileTimerRef.current = setTimeout(() => {
      const batch = [...profileQueueRef.current];
      profileQueueRef.current.clear();
      profileTimerRef.current = null;

      const relays = discoveryRelaysRef.current;
      if (batch.length === 0 || !relays?.length) return;

      fetchProfiles(relays, batch).then((fetched) => {
        setProfiles((prev) => {
          const next = new Map(prev);
          for (const [key, val] of fetched) {
            next.set(key, val);
          }
          return next;
        });
      });
    }, 500);
  }, []);

  const handleFetchOg = useCallback((url: string) => {
    if (ogPendingRef.current.has(url)) return;
    ogPendingRef.current.add(url);

    fetchOgTags(url)
      .then((data) => {
        setOgCache((prev) => {
          const next = new Map(prev);
          next.set(url, data);
          return next;
        });
      })
      .catch(() => {
        setOgCache((prev) => {
          const next = new Map(prev);
          next.set(url, null);
          return next;
        });
      });
  }, []);

  // Connect to relay via Nostr protocol
  useEffect(() => {
    if (!relay?.subdomain || !status?.domain) return;

    const isLocalhost = status.domain === "localhost" || status.domain.endsWith(".localhost");
    const wsProto = !isLocalhost && window.location.protocol === "https:" ? "wss:" : "ws:";
    const relayUrl = `${wsProto}//${relay.subdomain}.${status.domain}`;
    const subId = "admin-feed";

    let ws: WebSocket | null = null;

    // Defer connection so React Strict Mode's unmount/remount cycle
    // clears the timeout before a connection is ever opened
    const timer = setTimeout(() => {
      console.log("[RelayFeed] connecting to", relayUrl);
      ws = new WebSocket(relayUrl);
      wsRef.current = ws;
      gotEoseRef.current = false;

      ws.onopen = () => {
        console.log("[RelayFeed] connected");
        setConnected(true);
        ws!.send(JSON.stringify(["REQ", subId, { limit: 50 }]));
      };

      ws.onclose = (e) => {
        console.log("[RelayFeed] closed", e.code, e.reason);
        setConnected(false);
      };

      ws.onerror = (e) => {
        console.error("[RelayFeed] error", e);
      };

      ws.onmessage = (msg) => {
        try {
          const data = JSON.parse(msg.data);
          if (!Array.isArray(data)) return;

          const [type] = data;

          if (type === "EVENT" && data[2]) {
            const evt = data[2];
            const feedEvent: FeedEvent = {
              id: evt.id,
              pubkey: evt.pubkey,
              created_at: evt.created_at,
              kind: evt.kind,
              content: evt.content,
              tags: evt.tags,
            };

            if (gotEoseRef.current) {
              setEvents((prev) => {
                const next = [feedEvent, ...prev];
                if (next.length > MAX_EVENTS) next.length = MAX_EVENTS;
                return next;
              });
            } else {
              setEvents((prev) => {
                if (prev.some((e) => e.id === feedEvent.id)) return prev;
                const next = [...prev, feedEvent];
                if (next.length > MAX_EVENTS) next.length = MAX_EVENTS;
                return next;
              });
            }

            queueProfileFetch([feedEvent.pubkey]);
          } else if (type === "EOSE") {
            gotEoseRef.current = true;
          }
        } catch {
          // ignore parse errors
        }
      };
    }, 0);

    return () => {
      clearTimeout(timer);
      if (ws) {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify(["CLOSE", subId]));
        }
        ws.close();
        wsRef.current = null;
      }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [relay?.subdomain, status?.domain]);

  // Auto-scroll detection
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    function handleScroll() {
      if (!container) return;
      autoScrollRef.current = container.scrollTop < 100;
    }

    container.addEventListener("scroll", handleScroll);
    return () => container.removeEventListener("scroll", handleScroll);
  }, []);

  // Auto-scroll when new events arrive
  useEffect(() => {
    if (autoScrollRef.current && containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
  }, [events.length]);

  function handleDeleted(eventId: string) {
    setEvents((prev) => prev.filter((e) => e.id !== eventId));
  }

  if (!relay || !status) {
    return <p className="text-sm text-muted-foreground">Loading...</p>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {connected ? (
            <Wifi className="h-4 w-4 text-green-500" />
          ) : (
            <WifiOff className="h-4 w-4 text-destructive" />
          )}
          <span className="text-sm text-muted-foreground">
            {connected ? "Connected" : "Disconnected"} &middot; {events.length}{" "}
            events
          </span>
        </div>
      </div>

      <div
        ref={containerRef}
        className="space-y-3 max-h-[70vh] overflow-y-auto pr-1"
      >
        {events.length === 0 && connected && (
          <p className="text-sm text-muted-foreground text-center py-8">
            No events yet. Waiting for activity...
          </p>
        )}
        {events.map((event) => (
          <EventCard
            key={event.id}
            event={event}
            profile={profiles.get(event.pubkey)}
            relayId={relayId}
            ogCache={ogCache}
            onFetchOg={handleFetchOg}
            onDeleted={handleDeleted}
          />
        ))}
      </div>
    </div>
  );
}
