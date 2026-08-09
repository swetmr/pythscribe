import { useRouter } from "next/router";
export default function HomePage() {
    let router = useRouter();
    return null;
}
export async function getServerSideProps(context) {
    return ({"props": ({"data": "hello"})});
}
