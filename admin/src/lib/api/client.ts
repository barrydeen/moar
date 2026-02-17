export class ApiError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export async function apiFetch<T>(
  path: string,
  options?: RequestInit
): Promise<T> {
  const res = await fetch(`/api${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...options?.headers,
    },
    credentials: "include",
  });

  if (res.status === 401) {
    window.location.href = "/";
    throw new ApiError(401, "Unauthorized");
  }

  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }

  if (res.status === 204) {
    return undefined as T;
  }

  const contentType = res.headers.get("content-type") || "";
  if (contentType.includes("application/json")) {
    return res.json();
  }

  return undefined as T;
}

export async function apiUpload<T>(
  path: string,
  body: FormData
): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method: "POST",
    body,
    credentials: "include",
  });

  if (res.status === 401) {
    window.location.href = "/";
    throw new ApiError(401, "Unauthorized");
  }

  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }

  return res.json();
}
