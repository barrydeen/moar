"use client";

import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { TagInput } from "@/components/shared/tag-input";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import { useCreateBlossom, useUpdateBlossom } from "@/lib/hooks/use-blossoms";
import { blossomFormSchema, type BlossomFormData } from "@/lib/utils/validation";
import type { Blossom } from "@/lib/types/blossom";
import { toast } from "sonner";

function validatePubkey(value: string): string | null {
  if (!/^[0-9a-fA-F]{64}$/.test(value)) return "Must be a 64-character hex pubkey";
  return null;
}

interface BlossomFormProps {
  blossom?: Blossom;
}

export function BlossomForm({ blossom }: BlossomFormProps) {
  const router = useRouter();
  const createBlossom = useCreateBlossom();
  const updateBlossom = useUpdateBlossom();
  const isEdit = !!blossom;

  const {
    register,
    handleSubmit,
    watch,
    setValue,
    formState: { errors },
  } = useForm<BlossomFormData>({
    resolver: zodResolver(blossomFormSchema),
    defaultValues: blossom
      ? {
          id: blossom.id,
          name: blossom.name,
          description: blossom.description || "",
          subdomain: blossom.subdomain,
          storage_path: blossom.storage_path,
          policy: {
            upload: {
              allowed_pubkeys: blossom.policy.upload.allowed_pubkeys || [],
            },
            list: {
              require_auth: blossom.policy.list.require_auth,
              allowed_pubkeys: blossom.policy.list.allowed_pubkeys || [],
            },
            max_file_size: blossom.policy.max_file_size ?? null,
          },
        }
      : {
          id: "",
          name: "",
          description: "",
          subdomain: "",
          storage_path: "",
          policy: {
            upload: { allowed_pubkeys: [] },
            list: { require_auth: false, allowed_pubkeys: [] },
            max_file_size: null,
          },
        },
  });

  const listAuth = watch("policy.list.require_auth");
  const uploadPubkeys = watch("policy.upload.allowed_pubkeys") || [];
  const listPubkeys = watch("policy.list.allowed_pubkeys") || [];

  const uploadSummary = uploadPubkeys.length > 0
    ? `${uploadPubkeys.length} allowed uploaders`
    : "Open uploads";

  const listSummary = [
    listAuth && "Auth required",
    listPubkeys.length > 0 && `${listPubkeys.length} allowed`,
  ].filter(Boolean).join(", ") || "Open access";

  async function onSubmit(data: BlossomFormData) {
    const config = {
      name: data.name,
      description: data.description || undefined,
      subdomain: data.subdomain,
      storage_path: data.storage_path,
      policy: {
        upload: {
          allowed_pubkeys: data.policy.upload.allowed_pubkeys?.length
            ? data.policy.upload.allowed_pubkeys
            : undefined,
        },
        list: {
          require_auth: data.policy.list.require_auth,
          allowed_pubkeys: data.policy.list.allowed_pubkeys?.length
            ? data.policy.list.allowed_pubkeys
            : undefined,
        },
        max_file_size: data.policy.max_file_size ?? undefined,
      },
    };

    try {
      if (isEdit) {
        await updateBlossom.mutateAsync({ id: data.id, config });
        toast.success("Blossom server updated");
      } else {
        await createBlossom.mutateAsync({ id: data.id, config });
        toast.success("Blossom server created");
      }
      router.push("/admin/blossoms");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Operation failed");
    }
  }

  const isPending = createBlossom.isPending || updateBlossom.isPending;

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl mx-auto">
      <CollapsibleSection title="Basic Info" defaultOpen>
        <div className="space-y-2">
          <Label htmlFor="id">Server ID</Label>
          <Input id="id" {...register("id")} disabled={isEdit} placeholder="media" />
          {errors.id && <p className="text-xs text-destructive">{errors.id.message}</p>}
        </div>

        <div className="space-y-2">
          <Label htmlFor="name">Display Name</Label>
          <Input id="name" {...register("name")} placeholder="Media Server" />
          {errors.name && <p className="text-xs text-destructive">{errors.name.message}</p>}
        </div>

        <div className="space-y-2">
          <Label htmlFor="description">Description</Label>
          <Input id="description" {...register("description")} placeholder="Optional description" />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="subdomain">Subdomain</Label>
            <Input id="subdomain" {...register("subdomain")} placeholder="media" />
            {errors.subdomain && (
              <p className="text-xs text-destructive">{errors.subdomain.message}</p>
            )}
          </div>
          <div className="space-y-2">
            <Label htmlFor="storage_path">Storage Path</Label>
            <Input id="storage_path" {...register("storage_path")} placeholder="/app/data/blossom" />
            {errors.storage_path && (
              <p className="text-xs text-destructive">{errors.storage_path.message}</p>
            )}
          </div>
        </div>
      </CollapsibleSection>

      <CollapsibleSection title="Upload Policy" summary={uploadSummary}>
        <div className="space-y-2">
          <Label>Allowed Uploaders</Label>
          <TagInput
            values={uploadPubkeys}
            onChange={(v) => setValue("policy.upload.allowed_pubkeys", v)}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
          <p className="text-xs text-muted-foreground">Leave empty for open uploads</p>
        </div>
      </CollapsibleSection>

      <CollapsibleSection title="List Policy" summary={listSummary}>
        <div className="flex items-center justify-between rounded-md bg-muted/50 p-3">
          <Label htmlFor="list-auth">Require Authentication to List</Label>
          <Switch
            id="list-auth"
            checked={listAuth}
            onCheckedChange={(v) => setValue("policy.list.require_auth", v)}
          />
        </div>

        <div className="space-y-2">
          <Label>Allowed Listers</Label>
          <TagInput
            values={listPubkeys}
            onChange={(v) => setValue("policy.list.allowed_pubkeys", v)}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
        </div>
      </CollapsibleSection>

      <CollapsibleSection title="Limits" summary="File size constraints">
        <div className="space-y-2">
          <Label htmlFor="max_file_size">Max File Size (bytes)</Label>
          <Input
            id="max_file_size"
            type="number"
            {...register("policy.max_file_size")}
            placeholder="No limit"
          />
        </div>
      </CollapsibleSection>

      <div className="flex gap-3 pt-2">
        <Button type="button" variant="outline" onClick={() => router.push("/admin/blossoms")}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? "Saving..." : isEdit ? "Update Server" : "Create Server"}
        </Button>
      </div>
    </form>
  );
}
