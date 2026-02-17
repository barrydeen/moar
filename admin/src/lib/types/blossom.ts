export interface BlossomUploadPolicy {
  allowed_pubkeys?: string[] | null;
}

export interface BlossomListPolicy {
  require_auth: boolean;
  allowed_pubkeys?: string[] | null;
}

export interface BlossomPolicyConfig {
  upload: BlossomUploadPolicy;
  list: BlossomListPolicy;
  max_file_size?: number | null;
}

export interface BlossomConfig {
  name: string;
  description?: string | null;
  subdomain: string;
  storage_path: string;
  policy: BlossomPolicyConfig;
}

export interface Blossom {
  id: string;
  name: string;
  description?: string | null;
  subdomain: string;
  storage_path: string;
  policy: BlossomPolicyConfig;
}

export interface BlobDescriptor {
  url: string;
  sha256: string;
  size: number;
  type: string;
  uploaded: number;
}
