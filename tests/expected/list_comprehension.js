let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let evens = numbers.filter((x) => ((((x % 2) + 2) % 2) === 0)).map((x) => x);
let squares = numbers.map((x) => (x ** 2));
console.log(evens);
console.log(squares);
