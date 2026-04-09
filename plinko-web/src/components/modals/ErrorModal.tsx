import { Modal } from "../Modal";

interface Props {
  message: string;
  onClose: () => void;
}

export function ErrorModal({ message, onClose }: Props) {
  return (
    <Modal title="Error" onClose={onClose} width={380}>
      <p style={{ color: "#e57373", fontSize: 14, margin: "0 0 18px" }}>{message}</p>
      <div className="form-actions">
        <button className="btn btn-primary" onClick={onClose}>OK</button>
      </div>
    </Modal>
  );
}
