export function App() {
  return (
    <form>
      <div><label>{`name`}</label><input name={`name`} /></div>
      <div><label>{`email`}</label><input name={`email`} /></div>
      <div><label>{`age`}</label><input name={`age`} /></div>
    </form>
  );
}
