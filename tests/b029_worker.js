import { handler, Response } from "pyths-runtime/web";
export async function my_fetch(request) {
    return new Response("hello from worker", ({"status": 200}));
}
export default handler(my_fetch);
