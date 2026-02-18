import { redirect } from "next/navigation";

export default async function MediaPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  redirect(`/admin/blossoms/${id}/edit`);
}
