import { Badge } from "@/components/ui/badge";
import type { PolicyConfig } from "@/lib/types/relay";

export function PolicyBadges({ policy }: { policy: PolicyConfig }) {
  const badges: { label: string; variant: "default" | "secondary" | "outline" | "warning" }[] = [];

  if (policy.write.require_auth) {
    badges.push({ label: "Write Auth", variant: "secondary" });
  }
  if (policy.read.require_auth) {
    badges.push({ label: "Read Auth", variant: "secondary" });
  }
  if (policy.write.allowed_pubkeys?.length) {
    badges.push({ label: `${policy.write.allowed_pubkeys.length} writers`, variant: "outline" });
  }
  if (policy.write.tagged_pubkeys?.length) {
    badges.push({ label: "Tagged", variant: "outline" });
  }
  if (policy.write.wot) {
    badges.push({ label: `WoT: ${policy.write.wot}`, variant: "default" });
  }
  if (policy.read.wot) {
    badges.push({ label: `Read WoT: ${policy.read.wot}`, variant: "default" });
  }
  if (policy.events.allowed_kinds?.length) {
    badges.push({ label: `${policy.events.allowed_kinds.length} kinds`, variant: "outline" });
  }
  if (policy.rate_limit) {
    badges.push({ label: "Rate limited", variant: "warning" });
  }

  if (badges.length === 0) {
    badges.push({ label: "Public", variant: "secondary" });
  }

  return (
    <div className="flex flex-wrap gap-1">
      {badges.map((b, i) => (
        <Badge key={i} variant={b.variant} className="text-xs">
          {b.label}
        </Badge>
      ))}
    </div>
  );
}
