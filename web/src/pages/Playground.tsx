import { useState, useRef, useEffect } from 'react'
import {
  Card, Input, Button, Select, Space, Typography, Slider, Switch, Tag, App, Upload, Image,
} from 'antd'
import { SendOutlined, ClearOutlined, LoadingOutlined, PictureOutlined, CloseCircleFilled } from '@ant-design/icons'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { listProviders } from '../api/providers'
import { listProviderModels, listModelsMetadata } from '../api/models'

const { TextArea } = Input
const { Text } = Typography

interface ImageAttachment {
  base64: string
  mediaType: string
  name: string
}

interface Message {
  role: 'system' | 'user' | 'assistant'
  content: string
  reasoning?: string
  images?: ImageAttachment[]
}

export default function Playground() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [model, setModel] = useState('')
  const [customModel, setCustomModel] = useState('')
  const [systemPrompt, setSystemPrompt] = useState('')
  const [messages, setMessages] = useState<Message[]>([])
  const [input, setInput] = useState('')
  const [streaming, setStreaming] = useState(false)
  const [streamContent, setStreamContent] = useState('')
  const [streamReasoning, setStreamReasoning] = useState('')
  const [temperature, setTemperature] = useState<number | null>(0.7)
  const [maxTokens, setMaxTokens] = useState(2048)
  const [streamEnabled, setStreamEnabled] = useState(true)
  const [thinkingEnabled, setThinkingEnabled] = useState(false)
  const [thinkingBudget, setThinkingBudget] = useState(8192)
  const [apiKey, setApiKey] = useState('')
  const [availableModels, setAvailableModels] = useState<string[]>([])
  const [providers, setProviders] = useState<{ id: number; name: string; display_name: string | null }[]>([])
  const [selectedProviderId, setSelectedProviderId] = useState<number | undefined>(undefined)
  const [selectedProviderName, setSelectedProviderName] = useState<string | undefined>(undefined)
  const [apiFormat, setApiFormat] = useState<'openai' | 'anthropic' | 'responses'>('openai')
  const abortRef = useRef<AbortController | null>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [attachedImages, setAttachedImages] = useState<ImageAttachment[]>([])

  useEffect(() => {
    fetchProviders()
    loadAllModels()
  }, [])

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, streamContent, streamReasoning])

  useEffect(() => {
    if (selectedProviderId) {
      loadProviderModels(selectedProviderId)
    } else {
      loadAllModels()
    }
  }, [selectedProviderId])

  const fetchProviders = async () => {
    try {
      const res = await listProviders(1, 200)
      const items = res.data?.elements || res.data || []
      const list = Array.isArray(items) ? items : []
      setProviders(list.filter((p: any) => p.status === 1 && (!p.category || p.category === 'llm')).map((p: any) => ({
        id: p.id, name: p.name, display_name: p.display_name
      })))
    } catch { /* ignore */ }
  }

  const loadProviderModels = async (providerId: number) => {
    try {
      const res = await listProviderModels(providerId)
      const items = res.data || []
      const patterns = Array.isArray(items) ? items
        .filter((m: any) => m.enabled)
        .map((m: any) => m.model_pattern) : []

      if (patterns.includes('*')) {
        loadAllModels()
        return
      }

      setAvailableModels(patterns)
      if (patterns.length > 0 && !patterns.includes(model)) {
        setModel(patterns[0])
      }
    } catch { /* ignore */ }
  }

  const loadAllModels = async () => {
    try {
      const res = await listModelsMetadata()
      const items = res.data || []
      const unique = Array.isArray(items) ? items.map((m: any) => m.model_name) : []
      setAvailableModels(unique)
      if (unique.length > 0 && !model) {
        setModel(unique[0])
      }
    } catch { /* ignore */ }
  }

  const getEffectiveModel = () => customModel || model

  const handleImageSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files
    if (files) addImageFiles(files)
    e.target.value = ''
  }

  const removeImage = (idx: number) => {
    setAttachedImages(prev => prev.filter((_, i) => i !== idx))
  }

  /** Build multimodal content array for the user message */
  const buildUserContent = (text: string, images: ImageAttachment[]) => {
    if (images.length === 0) return text
    if (apiFormat === 'anthropic') {
      const parts: any[] = images.map(img => ({
        type: 'image',
        source: { type: 'base64', media_type: img.mediaType, data: img.base64 },
      }))
      parts.push({ type: 'text', text })
      return parts
    }
    // OpenAI / Responses format
    const parts: any[] = images.map(img => ({
      type: 'image_url',
      image_url: { url: `data:${img.mediaType};base64,${img.base64}` },
    }))
    parts.push({ type: 'text', text })
    return parts
  }

  const handleSend = async () => {
    const userMsg = input.trim() || (attachedImages.length > 0 ? t('playground.describeImage', '请描述这张图片') : '')
    if ((!userMsg && attachedImages.length === 0) || streaming) return

    const currentImages = [...attachedImages]
    const newMessages: Message[] = [...messages, { role: 'user', content: userMsg, images: currentImages.length > 0 ? currentImages : undefined }]
    setMessages(newMessages)
    setInput('')
    setAttachedImages([])
    setStreaming(true)
    setStreamContent('')
    setStreamReasoning('')

    const fullMessages: { role: string; content: any }[] = []
    if (systemPrompt) {
      fullMessages.push({ role: 'system', content: systemPrompt })
    }
    // Build messages with multimodal content when images are attached
    fullMessages.push(...newMessages.map(m => ({
      role: m.role,
      content: m.images?.length ? buildUserContent(m.content, m.images) : m.content,
    })))

    const body: Record<string, any> = {
      model: getEffectiveModel(),
      messages: fullMessages,
      max_tokens: maxTokens,
      stream: streamEnabled,
    }

    if (!thinkingEnabled && temperature !== null) {
      body.temperature = temperature
    }

    if (thinkingEnabled) {
      // OpenAI-compatible reasoning params (works for DeepSeek, o1/o3, etc.)
      body.reasoning_effort = 'medium'
      // Also include Anthropic-style thinking for Claude models
      body.thinking = { type: 'enabled', budget_tokens: thinkingBudget }
    }

    const headers: Record<string, string> = { 'Content-Type': 'application/json' }
    if (apiKey) headers['Authorization'] = `Bearer ${apiKey}`
    if (selectedProviderName) headers['X-Provider-Name'] = selectedProviderName

    // Choose endpoint path based on API format
    const endpointPath = apiFormat === 'anthropic' ? '/v1/messages'
      : apiFormat === 'responses' ? '/v1/responses'
      : '/v1/chat/completions'

    // If Anthropic format, restructure the body
    let requestBody: any
    if (apiFormat === 'anthropic') {
      headers['anthropic-version'] = '2023-06-01'
      const anthropicMessages = fullMessages
        .filter(m => m.role !== 'system')
        .map(m => ({ role: m.role, content: m.content }))
      requestBody = {
        model: getEffectiveModel(),
        messages: anthropicMessages,
        max_tokens: maxTokens,
        stream: streamEnabled,
      }
      if (systemPrompt) {
        requestBody.system = systemPrompt
      }
      if (thinkingEnabled) {
        requestBody.thinking = { type: 'enabled', budget_tokens: thinkingBudget }
      }
    } else if (apiFormat === 'responses') {
      // OpenAI Responses API format: input is the last user message or full conversation
      const lastUserMsg = newMessages.filter(m => m.role === 'user').pop()
      requestBody = {
        model: getEffectiveModel(),
        input: fullMessages.length > 1
          ? fullMessages.map(m => ({ role: m.role, content: m.content }))
          : lastUserMsg?.content || '',
        stream: streamEnabled,
      }
      if (systemPrompt) {
        requestBody.instructions = systemPrompt
      }
      if (maxTokens) {
        requestBody.max_output_tokens = maxTokens
      }
      if (!thinkingEnabled && temperature !== null) {
        requestBody.temperature = temperature
      }
    } else {
      requestBody = body
    }

    const controller = new AbortController()
    abortRef.current = controller

    try {
      const res = await fetch(endpointPath, {
        method: 'POST',
        headers,
        body: JSON.stringify(requestBody),
        signal: controller.signal,
      })

      if (!res.ok) {
        const err = await res.text()
        // Extract meaningful error message
        let errorMsg = `Error ${res.status}`
        try {
          const errJson = JSON.parse(err)
          const innerMsg = errJson?.error?.message || errJson?.message || ''
          if (innerMsg) {
            errorMsg += `: ${innerMsg.slice(0, 100)}`
          }
        } catch {
          if (err.length > 0) {
            errorMsg += `: ${err.slice(0, 100)}`
          }
        }
        message.error(errorMsg)
        setStreaming(false)
        return
      }

      if (streamEnabled && res.body) {
        const reader = res.body.getReader()
        const decoder = new TextDecoder()
        let accumulated = ''
        let accumulatedReasoning = ''
        let buffer = ''

        while (true) {
          const { done, value } = await reader.read()
          if (done) break

          buffer += decoder.decode(value, { stream: true })
          const lines = buffer.split(/\r?\n/)
          buffer = lines.pop() || '' // keep incomplete line in buffer

          for (const line of lines) {
            if (!line.startsWith('data: ')) continue
            const data = line.slice(6).trim()
            if (!data || data === '[DONE]') continue
            try {
              const json = JSON.parse(data)

              if (apiFormat === 'anthropic') {
                // Anthropic SSE format
                if (json.type === 'content_block_delta') {
                  if (json.delta?.type === 'text_delta' && json.delta.text) {
                    accumulated += json.delta.text
                    setStreamContent(accumulated)
                  }
                  if (thinkingEnabled && json.delta?.type === 'thinking_delta' && json.delta.thinking) {
                    accumulatedReasoning += json.delta.thinking
                    setStreamReasoning(accumulatedReasoning)
                  }
                } else if (json.type === 'message_delta' && json.delta?.stop_reason) {
                  // message complete
                }
              } else if (apiFormat === 'responses') {
                  // OpenAI Responses API SSE format
                  if (json.type === 'response.output_text.delta') {
                    accumulated += json.delta || ''
                    setStreamContent(accumulated)
                  }
                  if (thinkingEnabled && json.type === 'response.reasoning.delta') {
                    accumulatedReasoning += json.delta || ''
                    setStreamReasoning(accumulatedReasoning)
                  }
                } else {
                  // OpenAI SSE format
                  const delta = json.choices?.[0]?.delta
                  if (delta?.content) {
                    accumulated += delta.content
                    setStreamContent(accumulated)
                  }
                  if (thinkingEnabled) {
                    if (delta?.reasoning_content) {
                      accumulatedReasoning += delta.reasoning_content
                      setStreamReasoning(accumulatedReasoning)
                    }
                    if (delta?.reasoning) {
                      accumulatedReasoning += delta.reasoning
                      setStreamReasoning(accumulatedReasoning)
                    }
                  }
                }
              } catch (e) { console.warn('[SSE parse]', data, e) }
          }
        }

        setMessages([...newMessages, { role: 'assistant', content: accumulated, reasoning: accumulatedReasoning || undefined }])
        setStreamContent('')
        setStreamReasoning('')
      } else {
        const json = await res.json()
        let content = ''
        let reasoning: string | undefined

        if (apiFormat === 'anthropic') {
          // Anthropic non-streaming response
          content = json.content
            ?.filter((b: any) => b.type === 'text')
            .map((b: any) => b.text)
            .join('') || ''
          if (thinkingEnabled) {
            reasoning = json.content
              ?.filter((b: any) => b.type === 'thinking')
              .map((b: any) => b.thinking)
              .join('') || undefined
          }
        } else if (apiFormat === 'responses') {
          // OpenAI Responses API non-streaming
          content = json.output
            ?.filter((item: any) => item.type === 'message')
            .flatMap((item: any) => item.content || [])
            .filter((c: any) => c.type === 'output_text')
            .map((c: any) => c.text)
            .join('') || ''
        } else {
          // OpenAI non-streaming response
          content = json.choices?.[0]?.message?.content || ''
          reasoning = thinkingEnabled
            ? (json.choices?.[0]?.message?.reasoning_content || json.choices?.[0]?.message?.reasoning || undefined)
            : undefined
        }

        setMessages([...newMessages, { role: 'assistant', content, reasoning }])
      }
    } catch (e: any) {
      if (e.name !== 'AbortError') {
        message.error(`Request failed: ${e.message}`)
      }
    } finally {
      setStreaming(false)
      abortRef.current = null
    }
  }

  const handleStop = () => {
    abortRef.current?.abort()
    if (streamContent || streamReasoning) {
      setMessages((prev) => [...prev, { role: 'assistant', content: streamContent, reasoning: streamReasoning || undefined }])
      setStreamContent('')
      setStreamReasoning('')
    }
    setStreaming(false)
  }

  const handleClear = () => {
    setMessages([])
    setStreamContent('')
    setAttachedImages([])
  }

  const addImageFiles = (files: FileList | File[]) => {
    Array.from(files).forEach(file => {
      if (!file.type.startsWith('image/')) return
      const reader = new FileReader()
      reader.onload = () => {
        const result = reader.result as string
        const base64 = result.split(',')[1]
        setAttachedImages(prev => [...prev, { base64, mediaType: file.type, name: file.name }])
      }
      reader.readAsDataURL(file)
    })
  }

  const handlePaste = (e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items
    if (!items) return
    const imageFiles: File[] = []
    for (let i = 0; i < items.length; i++) {
      if (items[i].type.startsWith('image/')) {
        const file = items[i].getAsFile()
        if (file) imageFiles.push(file)
      }
    }
    if (imageFiles.length > 0) {
      e.preventDefault()
      addImageFiles(imageFiles)
    }
  }

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault()
    const files = e.dataTransfer?.files
    if (files) addImageFiles(files)
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  return (
    <div style={{ display: 'flex', height: '100%', gap: 12 }}>
      {/* Sidebar config */}
      <Card size="small" style={{ width: 260, flexShrink: 0, overflow: 'auto' }} styles={{ body: { padding: '12px 12px' } }}>
        <Space direction="vertical" style={{ width: '100%' }} size="small">
          <div>
            <Text strong style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>
              MRouter API Key
            </Text>
            <Input.Password
              size="small"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
            />
            <Text type="secondary" style={{ fontSize: 11 }}>{t('playground.apiKeyHint')}</Text>
          </div>

          <div style={{ borderTop: '1px solid #f0f0f0', paddingTop: 8 }}>
            <Text strong style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>
              {t('playground.provider')}
            </Text>
            <Select
              allowClear
              showSearch
              size="small"
              style={{ width: '100%' }}
              placeholder={t('playground.selectProvider')}
              value={selectedProviderId}
              onChange={(v) => {
                setSelectedProviderId(v)
                const p = providers.find((pp) => pp.id === v)
                setSelectedProviderName(p?.name)
              }}
              filterOption={(input, option) =>
                (option?.children as unknown as string)?.toLowerCase().includes(input.toLowerCase())
              }
            >
              {providers.map((p) => (
                <Select.Option key={p.id} value={p.id}>
                  {p.display_name || p.name}
                </Select.Option>
              ))}
            </Select>
          </div>

          <div>
            <Text strong style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>
              {t('playground.apiFormat', 'API 协议')}
            </Text>
            <Select
              size="small"
              style={{ width: '100%' }}
              value={apiFormat}
              onChange={setApiFormat}
            >
              <Select.Option value="openai">OpenAI (chat/completions)</Select.Option>
              <Select.Option value="anthropic">Anthropic (messages)</Select.Option>
              <Select.Option value="responses">OpenAI Responses</Select.Option>
            </Select>
          </div>

          <div>
            <Text strong style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>
              {t('playground.model')}
            </Text>
            <Select
              showSearch
              size="small"
              style={{ width: '100%' }}
              value={model || undefined}
              onChange={setModel}
              placeholder={t('playground.selectModel')}
            >
              {availableModels.map((m) => (
                <Select.Option key={m} value={m}>{m}</Select.Option>
              ))}
            </Select>
            <Input
              size="small"
              placeholder={t('playground.customModel')}
              value={customModel}
              onChange={(e) => setCustomModel(e.target.value)}
              style={{ marginTop: 4 }}
            />
          </div>

          <div>
            <Text strong style={{ fontSize: 12, display: 'block', marginBottom: 4 }}>
              {t('playground.systemPrompt')}
            </Text>
            <TextArea
              rows={3}
              size="small"
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              placeholder={t('playground.systemPromptHint')}
            />
          </div>

          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <Text strong style={{ fontSize: 12 }}>Temperature: {temperature ?? 'auto'}</Text>
              {temperature !== null && (
                <Button type="link" size="small" style={{ fontSize: 11, padding: 0 }} onClick={() => setTemperature(null)}>
                  auto
                </Button>
              )}
            </div>
            <Slider min={0} max={2} step={0.1} value={temperature ?? 0.7} onChange={(v) => setTemperature(v)} />
          </div>

          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <Text strong style={{ fontSize: 12 }}>Max Tokens: {maxTokens}</Text>
            </div>
            <Slider min={64} max={32768} step={64} value={maxTokens} onChange={setMaxTokens} />
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Switch size="small" checked={streamEnabled} onChange={setStreamEnabled} />
            <Text style={{ fontSize: 12 }}>Stream</Text>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Switch size="small" checked={thinkingEnabled} onChange={setThinkingEnabled} />
            <Text style={{ fontSize: 12 }}>{t('playground.thinking_mode')}</Text>
          </div>

          {thinkingEnabled && (
            <div>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <Text strong style={{ fontSize: 12 }}>{t('playground.thinkingBudget')}: {thinkingBudget}</Text>
              </div>
              <Slider min={1024} max={65536} step={1024} value={thinkingBudget} onChange={setThinkingBudget} />
            </div>
          )}
        </Space>
      </Card>

      {/* Chat area */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        {/* Messages */}
        <div
          style={{ flex: 1, overflow: 'auto', padding: '12px 16px', background: '#fff', borderRadius: 8, border: '1px solid #f0f0f0' }}
        >
          {messages.length === 0 && !streamContent && (
            <div style={{ textAlign: 'center', color: '#999', marginTop: 80 }}>
              {t('playground.emptyHint')}
            </div>
          )}
          {messages.map((msg, idx) => (
            <div
              key={idx}
              style={{
                display: 'flex',
                justifyContent: msg.role === 'user' ? 'flex-end' : 'flex-start',
                marginBottom: 12,
              }}
            >
              <div
                style={{
                  maxWidth: '75%',
                  padding: '10px 14px',
                  borderRadius: 12,
                  background: msg.role === 'user' ? '#1677ff' : '#f5f5f5',
                  color: msg.role === 'user' ? '#fff' : '#333',
                  fontSize: 14,
                  lineHeight: 1.6,
                  wordBreak: 'break-word',
                  overflow: 'hidden',
                }}
              >
                {msg.role === 'assistant' ? (
                  <div className="markdown-body">
                    {thinkingEnabled && msg.reasoning && (
                      <details style={{ marginBottom: 8, padding: '6px 10px', background: '#fafafa', borderRadius: 6, border: '1px solid #e8e8e8', fontSize: 12, color: '#666' }}>
                        <summary style={{ cursor: 'pointer', fontWeight: 500 }}>{t('playground.thinking_process')}</summary>
                        <pre style={{ whiteSpace: 'pre-wrap', marginTop: 4, fontSize: 12 }}>{msg.reasoning}</pre>
                      </details>
                    )}
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{msg.content}</ReactMarkdown>
                  </div>
                ) : (
                  <div>
                    {msg.images && msg.images.length > 0 && (
                      <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginBottom: 6 }}>
                        <Image.PreviewGroup>
                          {msg.images.map((img, i) => (
                            <Image
                              key={i}
                              src={`data:${img.mediaType};base64,${img.base64}`}
                              style={{ borderRadius: 6, objectFit: 'cover', cursor: 'pointer' }}
                              width={80}
                              height={80}
                            />
                          ))}
                        </Image.PreviewGroup>
                      </div>
                    )}
                    <span style={{ whiteSpace: 'pre-wrap' }}>{msg.content}</span>
                  </div>
                )}
              </div>
            </div>
          ))}
          {streaming && (streamContent || streamReasoning) && (
            <div style={{ display: 'flex', justifyContent: 'flex-start', marginBottom: 12 }}>
              <div
                style={{
                  maxWidth: '75%',
                  padding: '10px 14px',
                  borderRadius: 12,
                  background: '#f5f5f5',
                  color: '#333',
                  fontSize: 14,
                  lineHeight: 1.6,
                  wordBreak: 'break-word',
                }}
              >
                {streamReasoning && (
                  <details open style={{ marginBottom: 8, padding: '6px 10px', background: '#fafafa', borderRadius: 6, border: '1px solid #e8e8e8', fontSize: 12, color: '#666' }}>
                    <summary style={{ cursor: 'pointer', fontWeight: 500 }}>{t('playground.thinking_process')} <LoadingOutlined style={{ fontSize: 10 }} /></summary>
                    <pre style={{ whiteSpace: 'pre-wrap', marginTop: 4, fontSize: 12 }}>{streamReasoning}</pre>
                  </details>
                )}
                {streamContent && (
                  <div className="markdown-body">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{streamContent}</ReactMarkdown>
                  </div>
                )}
                <LoadingOutlined style={{ marginLeft: 4, fontSize: 12 }} />
              </div>
            </div>
          )}
          {streaming && !streamContent && !streamReasoning && (
            <div style={{ display: 'flex', justifyContent: 'flex-start', marginBottom: 12 }}>
              <Tag icon={<LoadingOutlined />} color="blue">{t('playground.thinking')}</Tag>
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        {/* Input */}
        <div style={{ marginTop: 8 }}>
          {attachedImages.length > 0 && (
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 6 }}>
              {attachedImages.map((img, i) => (
                <div key={i} style={{ position: 'relative', display: 'inline-block' }}>
                  <img
                    src={`data:${img.mediaType};base64,${img.base64}`}
                    alt={img.name}
                    style={{ height: 48, borderRadius: 6, border: '1px solid #d9d9d9' }}
                  />
                  <CloseCircleFilled
                    onClick={() => removeImage(i)}
                    style={{ position: 'absolute', top: -6, right: -6, fontSize: 16, color: '#ff4d4f', cursor: 'pointer', background: '#fff', borderRadius: '50%' }}
                  />
                </div>
              ))}
            </div>
          )}
          <div style={{ display: 'flex', gap: 8 }}>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/*"
              multiple
              style={{ display: 'none' }}
              onChange={handleImageSelect}
            />
            <Button
              icon={<PictureOutlined />}
              onClick={() => fileInputRef.current?.click()}
              disabled={streaming}
              title={t('playground.attachImage')}
            />
            <TextArea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              onPaste={handlePaste}
              onDrop={handleDrop}
              onDragOver={(e) => e.preventDefault()}
              placeholder={t('playground.inputPlaceholder')}
              autoSize={{ minRows: 1, maxRows: 4 }}
              disabled={streaming}
              style={{ flex: 1 }}
            />
            {streaming ? (
              <Button danger onClick={handleStop}>{t('playground.stop')}</Button>
            ) : (
              <Space.Compact>
                <Button type="primary" icon={<SendOutlined />} onClick={handleSend} disabled={!input.trim() && attachedImages.length === 0}>
                  {t('playground.send')}
                </Button>
                <Button icon={<ClearOutlined />} onClick={handleClear} />
              </Space.Compact>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
