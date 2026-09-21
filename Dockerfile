FROM node:22-alpine

WORKDIR /app

# Copy initial files (overridden by volumes at runtime)
COPY dashboard/ ./dashboard/
COPY sites/ ./sites/

# Dashboard port (site ports are dynamic — check sites.json)
EXPOSE 4445

CMD ["node", "dashboard/server.js"]
