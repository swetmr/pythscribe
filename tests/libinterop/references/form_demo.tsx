// react-hook-form — TSX reference (oracle).
import * as React from 'react'
import { useForm, Controller } from 'react-hook-form'

export function FormDemo() {
  const rhf = useForm({ mode: 'onSubmit' })
  const [submitted, setSubmitted] = React.useState<string | null>(null)

  const onValid = (data: any) => {
    setSubmitted(`${data.email}|${data.nickname}`)
  }

  const errors = rhf.formState.errors
  return (
    <form onSubmit={rhf.handleSubmit(onValid)}>
      <input data-testid="email" {...rhf.register('email', { required: 'Email required' })} />
      {errors.email ? (
        <p data-testid="email-error">{String(errors.email.message)}</p>
      ) : null}
      <Controller
        control={rhf.control}
        name="nickname"
        defaultValue=""
        rules={{ required: 'Nickname required' }}
        render={({ field }) => <input data-testid="nickname" {...field} />}
      />
      {errors.nickname ? (
        <p data-testid="nickname-error">{String(errors.nickname.message)}</p>
      ) : null}
      <p data-testid="submitted">{submitted ?? 'none'}</p>
      <button type="submit">Send</button>
    </form>
  )
}
