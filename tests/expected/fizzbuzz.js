import { pyRange } from "pyths-runtime";

for (const i of pyRange(1, 21)) {
    if (((((i % 15) + 15) % 15) === 0)) {
        console.log("FizzBuzz");
    } else if (((((i % 3) + 3) % 3) === 0)) {
        console.log("Fizz");
    } else if (((((i % 5) + 5) % 5) === 0)) {
        console.log("Buzz");
    } else {
        console.log(i);
    }
}
