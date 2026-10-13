I need help implementing a collapse function in the sorting for monoarachy sort

here is the rules

Drums
-Drum Kit
--Kick
---In: [Kick In]

needs to just resolve to 

Kick: [Kick in]


Drums
-Drum Kit
--Kick
---In: [Kick In]
---Out: [Kick Out]

resolves to

Kick
-In: [Kick In]
-Out: [Kick Out]


Drums
-Drum Kit
--Kick
---In: [Kick In]
---Out: [Kick Out]
--Snare: [Snare Top]

resolves to

Drums
-Kick: 
--In: [Kick In]
--Out: [Kick Out]
-Snare: [Snare Top]

So we need to mimize the amount of folders, and be able to collapse a full hiherarchy into the least amount of needed folders, to maintain detail, we keep the Bottom level Groups (Kick) no matter what as those will fold or get placed correctly, but we need to keep the very top level group (Drums) always unless it resolves to not having more than one child, like only Kick, then in that case we remove it and were left with just the Kick Group