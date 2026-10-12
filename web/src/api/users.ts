import client from './client'

export const listUsers = () => client.get('/api/users')
export const createUser = (data: any) => client.post('/api/users', data)
export const updateUser = (id: number, data: any) => client.put(`/api/users/${id}`, data)
export const deleteUser = (id: number) => client.delete(`/api/users/${id}`)
