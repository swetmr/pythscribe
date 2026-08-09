// js-framework-benchmark — row-table app, KEYED, React reference track.
//
// Byte-identical DOM to the PythScribe `../Main.ps` implementation. This is the
// dual-track oracle: any divergence between this and the compiled `.ps` output
// is a PythScribe bug, not a benchmark artifact.
import { useState, useCallback } from 'react'

const ADJECTIVES = ['pretty', 'large', 'big', 'small', 'tall', 'short', 'long',
  'handsome', 'plain', 'quaint', 'clean', 'elegant', 'easy', 'angry', 'crazy',
  'helpful', 'mushy', 'odd', 'unsightly', 'adorable', 'important', 'inexpensive',
  'cheap', 'expensive', 'fancy']
const COLOURS = ['red', 'yellow', 'blue', 'green', 'pink', 'brown', 'purple',
  'brown', 'white', 'black', 'orange']
const NOUNS = ['table', 'chair', 'house', 'bbq', 'desk', 'car', 'pony', 'cookie',
  'sandwich', 'burger', 'pizza', 'mouse', 'keyboard']

type RowItem = { id: number; label: string }

let nextId = 1
const pick = <T,>(a: T[]) => a[Math.floor(Math.random() * a.length)]

function buildData(count: number): RowItem[] {
  const data: RowItem[] = new Array(count)
  for (let i = 0; i < count; i++) {
    data[i] = { id: nextId++, label: `${pick(ADJECTIVES)} ${pick(COLOURS)} ${pick(NOUNS)}` }
  }
  return data
}

function Row({ item, selected, sel, rem }: {
  item: RowItem; selected: boolean;
  sel: (id: number) => void; rem: (id: number) => void
}) {
  return (
    <tr className={selected ? 'danger' : ''}>
      <td className="col-md-1">{item.id}</td>
      <td className="col-md-4"><a onClick={() => sel(item.id)}>{item.label}</a></td>
      <td className="col-md-1"><a onClick={() => rem(item.id)}><span className="glyphicon glyphicon-remove" /></a></td>
      <td className="col-md-6" />
    </tr>
  )
}

function Button({ id, label, action }: { id: string; label: string; action: () => void }) {
  return (
    <div className="col-sm-6 smallpad">
      <button type="button" className="btn btn-primary btn-block" id={id} onClick={action}>{label}</button>
    </div>
  )
}

export default function App() {
  const [data, setData] = useState<RowItem[]>([])
  const [selected, setSelected] = useState(0)

  const run = useCallback(() => setData(buildData(1000)), [])
  const runLots = useCallback(() => setData(buildData(10000)), [])
  const add = useCallback(() => setData((d) => d.concat(buildData(1000))), [])
  const update = useCallback(() => setData((d) => {
    const u = d.slice()
    for (let i = 0; i < u.length; i += 10) u[i] = { id: u[i].id, label: u[i].label + ' !!!' }
    return u
  }), [])
  const clear = useCallback(() => setData([]), [])
  const swapRows = useCallback(() => setData((d) => {
    if (d.length <= 998) return d
    const u = d.slice()
    const tmp = u[1]; u[1] = u[998]; u[998] = tmp
    return u
  }), [])
  const select = useCallback((id: number) => setSelected(id), [])
  const remove = useCallback((id: number) => setData((d) => d.filter((x) => x.id !== id)), [])

  return (
    <div className="container">
      <div className="jumbotron">
        <div className="row">
          <div className="col-md-6"><h1>React-keyed</h1></div>
          <div className="col-md-6">
            <div className="row">
              <Button id="run" label="Create 1,000 rows" action={run} />
              <Button id="runlots" label="Create 10,000 rows" action={runLots} />
              <Button id="add" label="Append 1,000 rows" action={add} />
              <Button id="update" label="Update every 10th row" action={update} />
              <Button id="clear" label="Clear" action={clear} />
              <Button id="swaprows" label="Swap Rows" action={swapRows} />
            </div>
          </div>
        </div>
      </div>
      <table className="table table-hover table-striped test-data">
        <tbody>
          {data.map((item) => (
            <Row key={item.id} item={item} selected={item.id === selected} sel={select} rem={remove} />
          ))}
        </tbody>
      </table>
      <span className="preloadicon glyphicon glyphicon-remove" />
    </div>
  )
}
