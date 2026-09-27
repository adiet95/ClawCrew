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
}
