import * as React from 'react'

import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs'

export const Default = () => (
  <Tabs defaultValue="orders">
    <TabsList>
      <TabsTrigger value="orders">[ ORDERS ]</TabsTrigger>
      <TabsTrigger value="fills">[ FILLS ]</TabsTrigger>
      <TabsTrigger value="positions">[ POSITIONS ]</TabsTrigger>
    </TabsList>
    <TabsContent value="orders">
      <p className="font-mono text-[11px] uppercase tracking-[0.15em] text-brand-muted">
        No open orders.
      </p>
    </TabsContent>
    <TabsContent value="fills">
      <p className="font-mono text-[11px] uppercase tracking-[0.15em] text-brand-muted">
        No fills yet.
      </p>
    </TabsContent>
    <TabsContent value="positions">
      <p className="font-mono text-[11px] uppercase tracking-[0.15em] text-brand-muted">
        No positions.
      </p>
    </TabsContent>
  </Tabs>
)
