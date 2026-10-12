import client from './client'

export const listOrganizations = () => client.get('/api/organizations')
export const createOrganization = (data: { name: string; display_name?: string }) =>
  client.post('/api/organizations', data)
export const updateOrganization = (id: number, data: Record<string, any>) =>
  client.put(`/api/organizations/${id}`, data)
