import { z } from "zod";

const idSchema = z
  .string()
  .min(1, "ID is required")
  .regex(/^[a-zA-Z0-9_-]+$/, "Only alphanumeric, hyphens, and underscores");

const subdomainSchema = z.string().min(1, "Subdomain is required");

const pubkeySchema = z
  .string()
  .length(64, "Must be 64-character hex pubkey")
  .regex(/^[0-9a-fA-F]+$/, "Must be valid hex");

const pubkeyListSchema = z.array(pubkeySchema).optional();
const kindListSchema = z.array(z.coerce.number().int().min(0)).optional();

export const relayFormSchema = z.object({
  id: idSchema,
  name: z.string().min(1, "Name is required"),
  description: z.string().optional(),
  subdomain: subdomainSchema,
  db_path: z.string().min(1, "Database path is required"),
  policy: z.object({
    write: z.object({
      require_auth: z.boolean(),
      allowed_pubkeys: pubkeyListSchema,
      blocked_pubkeys: pubkeyListSchema,
      tagged_pubkeys: pubkeyListSchema,
      wot: z.string().nullable().optional(),
    }),
    read: z.object({
      require_auth: z.boolean(),
      allowed_pubkeys: pubkeyListSchema,
      wot: z.string().nullable().optional(),
    }),
    events: z.object({
      allowed_kinds: kindListSchema,
      blocked_kinds: kindListSchema,
      min_pow: z.coerce.number().int().min(0).nullable().optional(),
      max_content_length: z.coerce.number().int().min(0).nullable().optional(),
    }),
    rate_limit: z
      .object({
        writes_per_minute: z.coerce.number().int().min(1).nullable().optional(),
        reads_per_minute: z.coerce.number().int().min(1).nullable().optional(),
      })
      .nullable()
      .optional(),
  }),
});

export type RelayFormData = z.infer<typeof relayFormSchema>;

export const blossomFormSchema = z.object({
  id: idSchema,
  name: z.string().min(1, "Name is required"),
  description: z.string().optional(),
  subdomain: subdomainSchema,
  storage_path: z.string().min(1, "Storage path is required"),
  policy: z.object({
    upload: z.object({
      allowed_pubkeys: pubkeyListSchema,
    }),
    list: z.object({
      require_auth: z.boolean(),
      allowed_pubkeys: pubkeyListSchema,
    }),
    max_file_size: z.coerce.number().int().min(0).nullable().optional(),
  }),
});

export type BlossomFormData = z.infer<typeof blossomFormSchema>;

export const wotFormSchema = z.object({
  id: idSchema,
  seed: pubkeySchema,
  depth: z.coerce.number().int().min(1).max(4),
  update_interval_hours: z.coerce.number().int().min(1),
});

export type WotFormData = z.infer<typeof wotFormSchema>;
