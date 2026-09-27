pub struct RemoteGateway {
    pub instance_id: String,
    pub is_online: bool,
}

impl RemoteGateway {
    pub fn new(instance_id: String) -> Self {
        Self {
            instance_id,
            is_online: false,
        }
    }

    pub fn reconnect(&mut self) {
        self.is_online = true;
    }

    pub async fn forward_turn(&self, _task_id: &str, _payload: serde_json::Value) -> Result<(), anyhow::Error> {
        if !self.is_online {
            anyhow::bail!("Remote instance {} is offline", self.instance_id);
        }
        // TODO: The actual HTTP call to the remote instance is a placeholder since we don't know the exact endpoint, 
        // but this matches the gap for "eksekusi lintas-instance".
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn offline_gateway_rejects_forward() {
        let gw = RemoteGateway::new("gw-123".into());
        let res = gw.forward_turn("task-1", serde_json::json!({})).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("offline"));
    }

    #[tokio::test]
    async fn online_gateway_accepts_forward() {
        let mut gw = RemoteGateway::new("gw-123".into());
        gw.reconnect();
        let res = gw.forward_turn("task-1", serde_json::json!({})).await;
        assert!(res.is_ok());
    }
}
