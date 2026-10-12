import client from './client'

export interface SearchTestResult {
  status: number
  response: any
  provider: string
}

export const listSearchProviders = () =>
  client.get('/api/search/providers')

export const testSearch = (data: { provider_id: number; query?: string; path?: string; body?: any }) =>
  client.post('/api/search/test', data)
