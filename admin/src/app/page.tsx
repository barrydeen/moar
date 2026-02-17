"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { hasNostrExtension, createLoginEvent } from "@/lib/utils/nostr";
import { login } from "@/lib/api/auth";
import { Zap } from "lucide-react";

export default function LoginPage() {
  const router = useRouter();
  const [hasExtension, setHasExtension] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Check after a short delay for extension to load
    const timer = setTimeout(() => {
      setHasExtension(hasNostrExtension());
    }, 200);
    return () => clearTimeout(timer);
  }, []);

  async function handleLogin() {
    setLoading(true);
    setError(null);
    try {
      const event = await createLoginEvent();
      await login(event);
      router.push("/admin");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle className="text-3xl font-bold tracking-tight">
            MOAR Admin
          </CardTitle>
          <CardDescription>
            Sign in with your Nostr identity to manage your relay gateway.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {hasExtension ? (
            <Button
              onClick={handleLogin}
              disabled={loading}
              className="w-full"
              size="lg"
            >
              <Zap className="mr-2 h-5 w-5" />
              {loading ? "Signing in..." : "Sign in with Nostr"}
            </Button>
          ) : (
            <div className="text-center space-y-3">
              <p className="text-sm text-muted-foreground">
                No Nostr extension detected. Install a NIP-07 browser extension
                to sign in.
              </p>
              <div className="flex flex-col gap-2 text-sm text-muted-foreground">
                <a
                  href="https://getalby.com"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-primary hover:underline"
                >
                  Alby
                </a>
                <a
                  href="https://github.com/nicehash/nos2x"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-primary hover:underline"
                >
                  nos2x
                </a>
              </div>
              <Button
                onClick={() => setHasExtension(hasNostrExtension())}
                variant="outline"
                className="mt-2"
              >
                Check again
              </Button>
            </div>
          )}
          {error && (
            <p className="text-sm text-destructive text-center">{error}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
