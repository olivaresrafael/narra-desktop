---
name: personajes
description: Usa este skill para crear o desarrollar fichas de personaje en la biblia — arco, voz, relaciones, motor dramático. Un archivo por personaje en biblia/personajes/.
---

# Personajes

## Objetivo
Construir fichas de personaje que un guionista (humano o IA) pueda
usar para escribir diálogo consistente y decisiones creíbles.

## Ficha estándar
Guarda cada personaje en `biblia/personajes/<nombre>.md`:

```markdown
# <Nombre>

## Rol en la historia
Protagonista / antagonista / secundario / etc. Qué función dramática cumple.

## Quiere / Necesita
Lo que el personaje cree que quiere (deseo consciente) vs. lo que
realmente necesita (arco). Pueden estar en conflicto.

## Arco
De dónde parte a dónde llega emocionalmente. Si el personaje NO
cambia (personajes de soporte cómico, por ejemplo), dilo explícito.

## Voz
Cómo habla: vocabulario, ritmo, muletillas, qué NO diría nunca.
2-3 ejemplos de línea de diálogo suya si ya existen.

## Relaciones
Con quién se relaciona y cómo (tensión, alianza, historia compartida).

## Detalles físicos y de producción
Edad, aspecto si es relevante para casting/producción.

## Reglas específicas
Cualquier restricción especial (ej: "nunca lo vemos hablar",
"solo se comunica a través de X").
```

## Proceso
1. Si el personaje ya existe en un guion previo (adaptación), extrae
   lo que el material original ya establece antes de inventar nada.
2. Pregunta al usuario lo que falta — no inventes arco o motivación
   sin confirmar, salvo que pidan que propongas opciones.
3. Verifica contra `biblia/premisa.md` que el personaje sirva al tema.
4. Si dos personajes tienen voces parecidas, señálalo — hay que
   diferenciarlas antes de escribir escenas.

## Qué NO hacer
- No le des a un personaje una revelación psicológica de la nada
  a mitad de escritura sin actualizar su ficha.
- No escribas diálogo de un personaje sin haber leído su ficha primero.
