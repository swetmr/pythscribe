// @tanstack/react-query — TSX reference (oracle).
import * as React from 'react'
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query'

function fetchGreeting(): Promise<{ message: string }> {
  return new Promise((resolve) => {
    setTimeout(() => resolve({ message: 'hello from query' }), 0)
  })
}

const client = new QueryClient({
  defaultOptions: { queries: { retry: false } },
})

function Inner() {
  const result = useQuery({ queryKey: ['greeting'], queryFn: fetchGreeting })
  if (result.isLoading) return <p data-testid="loading">loading...</p>
  if (result.isError) return <p data-testid="error">error</p>
  return <p data-testid="data">{result.data!.message}</p>
}

export function QueryDemo() {
  return (
    <QueryClientProvider client={client}>
      <Inner />
    </QueryClientProvider>
  )
}
