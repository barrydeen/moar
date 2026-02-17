import { redirect } from "next/navigation";

export default function MediaPage({
  params,
}: {
  params: { id: string };
}) {
  redirect(`/admin/blossoms/${params.id}/edit`);
}
