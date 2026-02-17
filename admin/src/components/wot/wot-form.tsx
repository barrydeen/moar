"use client";

import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { useCreateWot, useUpdateWot } from "@/lib/hooks/use-wot";
import { wotFormSchema, type WotFormData } from "@/lib/utils/validation";
import type { WotInfo } from "@/lib/types/wot";
import { toast } from "sonner";

interface WotFormProps {
  wot?: WotInfo;
}

export function WotForm({ wot }: WotFormProps) {
  const router = useRouter();
  const createWot = useCreateWot();
  const updateWot = useUpdateWot();
  const isEdit = !!wot;

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<WotFormData>({
    resolver: zodResolver(wotFormSchema),
    defaultValues: wot
      ? {
          id: wot.id,
          seed: wot.config.seed,
          depth: wot.config.depth,
          update_interval_hours: wot.config.update_interval_hours,
        }
      : {
          id: "",
          seed: "",
          depth: 1,
          update_interval_hours: 24,
        },
  });

  async function onSubmit(data: WotFormData) {
    try {
      if (isEdit) {
        await updateWot.mutateAsync({
          id: data.id,
          seed: data.seed,
          depth: data.depth,
          update_interval_hours: data.update_interval_hours,
        });
        toast.success("Web of Trust updated");
      } else {
        await createWot.mutateAsync(data);
        toast.success("Web of Trust created");
      }
      router.push("/admin/wot");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Operation failed");
    }
  }

  const isPending = createWot.isPending || updateWot.isPending;

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-6 max-w-xl">
      <div className="space-y-2">
        <Label htmlFor="id">WoT ID</Label>
        <Input id="id" {...register("id")} disabled={isEdit} placeholder="my-wot" />
        {errors.id && <p className="text-xs text-destructive">{errors.id.message}</p>}
      </div>

      <div className="space-y-2">
        <Label htmlFor="seed">Seed Pubkey</Label>
        <Input id="seed" {...register("seed")} placeholder="64-character hex pubkey" className="font-mono" />
        {errors.seed && <p className="text-xs text-destructive">{errors.seed.message}</p>}
        <p className="text-xs text-muted-foreground">
          The WoT is built from this pubkey&apos;s follow list
        </p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="depth">Depth</Label>
          <Input id="depth" type="number" {...register("depth")} min={1} max={4} />
          {errors.depth && <p className="text-xs text-destructive">{errors.depth.message}</p>}
          <p className="text-xs text-muted-foreground">1-4 hops from seed</p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="update_interval_hours">Update Interval (hours)</Label>
          <Input id="update_interval_hours" type="number" {...register("update_interval_hours")} min={1} />
          {errors.update_interval_hours && (
            <p className="text-xs text-destructive">{errors.update_interval_hours.message}</p>
          )}
        </div>
      </div>

      <div className="flex gap-3">
        <Button type="button" variant="outline" onClick={() => router.push("/admin/wot")}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? "Saving..." : isEdit ? "Update WoT" : "Create WoT"}
        </Button>
      </div>
    </form>
  );
}
