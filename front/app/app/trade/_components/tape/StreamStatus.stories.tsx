import type { Meta, StoryObj } from '@storybook/react'

import { StreamStatus } from './StreamStatus'

const meta: Meta<typeof StreamStatus> = {
  title: 'Trade/Tape/StreamStatus',
  component: StreamStatus,
}
export default meta

type Story = StoryObj<typeof StreamStatus>

export const Live: Story = { args: { status: 'live' } }
export const Connecting: Story = { args: { status: 'connecting' } }
export const Degraded: Story = { args: { status: 'degraded' } }
